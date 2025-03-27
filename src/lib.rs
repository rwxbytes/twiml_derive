extern crate proc_macro;


use proc_macro::TokenStream;
use quote::quote;
use syn::{self, DeriveInput, Type};

#[proc_macro_derive(ToTwiML, attributes(xml))]
pub fn to_twiml_derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let data = match input.data {
        syn::Data::Struct(ref data) => data,
        _ => panic!("ToTwiML can only be derived for structs"),
    };

    let mut text_field = None;       // For text content (e.g., String)
    let mut nested_field = None;     // For nested elements (e.g., Vec<T> or custom types)
    let mut attr_fields = Vec::new(); // For attributes

    // Process each field
    for field in data.fields.iter() {
        let field_name = field.ident.as_ref().expect("Fields must be named");
        let field_type = &field.ty;
        let mut xml_name = field_name.to_string();
        let mut is_attribute = false;
        let mut is_content = false;

        for attr in &field.attrs {
            if attr.path.is_ident("xml") {
                if let Ok(meta) = attr.parse_meta() {
                    match meta {
                        syn::Meta::List(list) => {
                            for nested in list.nested {
                                match nested {
                                    syn::NestedMeta::Meta(syn::Meta::NameValue(nv)) => {
                                        if nv.path.is_ident("attribute") {
                                            if let syn::Lit::Str(lit) = nv.lit {
                                                xml_name = lit.value();
                                                is_attribute = true;
                                            }
                                        }
                                    }
                                    syn::NestedMeta::Meta(syn::Meta::Path(path)) => {
                                        if path.is_ident("content") {
                                            is_content = true;
                                            if is_vec_or_option_vec(field_type) || is_custom_type(field_type) {
                                                nested_field = Some((field_name.clone(), field_type.clone()));
                                            } else {
                                                text_field = Some((field_name.clone(), field_type.clone()));
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if is_attribute {
            attr_fields.push((field_name.clone(), xml_name, field_type.clone()));
        }
    }

    // Generate attribute collectors
    let attr_collectors = attr_fields.iter().map(|(field, xml_name, field_type)| {
        if is_optional(field_type) {
            quote! {
                if let Some(value) = &self.#field {
                    attributes.push((#xml_name.to_string(), value.to_string()));
                }
            }
        } else {
            quote! {
                attributes.push((#xml_name.to_string(), self.#field.to_string()));
            }
        }
    });

    // Generate text content write logic
    let text_write = if let Some((text_field, field_type)) = text_field {
        if is_optional(&field_type) {
            quote! {
                if let Some(value) = &self.#text_field {
                    writer.write(::xml::writer::XmlEvent::Characters(value))?;
                }
            }
        } else {
            quote! {
                writer.write(::xml::writer::XmlEvent::Characters(&self.#text_field))?;
            }
        }
    } else {
        quote! {}
    };

    // Generate nested elements write logic
    let nested_write = if let Some((nested_field, field_type)) = nested_field {
        if is_vec_or_option_vec(&field_type) {
            if is_optional(&field_type) {
                quote! {
                    if let Some(items) = &self.#nested_field {
                        for item in items {
                            item.write_xml(writer)?;
                        }
                    }
                }
            } else {
                quote! {
                    for item in &self.#nested_field {
                        item.write_xml(writer)?;
                    }
                }
            }
        } else {
            // Custom type like Noun
            quote! {
                self.#nested_field.write_xml(writer)?;
            }
        }
    } else {
        quote! {}
    };

    // Generate the full implementation
    let expanded = quote! {
        impl ToTwiML for #name {
            fn write_xml(&self, writer: &mut ::xml::writer::EventWriter<Vec<u8>>) -> Result<(), TwilioError> {
                use ::xml::writer::{XmlEvent, EventWriter};
                let mut attributes = Vec::new();
                #(#attr_collectors)*

                let mut element = XmlEvent::start_element(stringify!(#name));
                for (key, value) in &attributes {
                    element = element.attr(key.as_str(), value.as_str());
                }
                writer.write(element)?;
                #text_write
                #nested_write
                writer.write(XmlEvent::end_element())?;
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

// Helper functions
fn is_optional(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn is_vec_or_option_vec(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                return true;
            } else if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(inner_type))) = args.args.first() {
                        if let Some(inner_segment) = inner_type.path.segments.last() {
                            return inner_segment.ident == "Vec";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_custom_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        let type_name = type_path.path.segments.last().unwrap().ident.to_string();
        !matches!(type_name.as_str(), "String" | "i32" | "bool" | "Option" | "Vec")
    } else {
        true
    }
}