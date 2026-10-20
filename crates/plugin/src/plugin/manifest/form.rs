use anyhow::{Error, ensure};
use domain::component::{
    ComboboxField, ComboboxOption, ComboboxOptions, ConfigSchema, ConfigSection, FormElement,
    LoginForm, TextField,
};
use serde::{Deserialize, Deserializer, de::Error as _};

pub(super) fn login_form<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<LoginForm>, D::Error> {
    Option::<LoginFormDef>::deserialize(deserializer)?
        .map(LoginForm::try_from)
        .transpose()
        .map_err(D::Error::custom)
}

pub(super) fn config_schema<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ConfigSchema>, D::Error> {
    Option::<ConfigSchemaDef>::deserialize(deserializer)?
        .map(ConfigSchema::try_from)
        .transpose()
        .map_err(D::Error::custom)
}

#[derive(Deserialize)]
struct LoginFormDef {
    elements: Vec<FormElementDef>,
}

impl TryFrom<LoginFormDef> for LoginForm {
    type Error = Error;

    fn try_from(def: LoginFormDef) -> Result<Self, Self::Error> {
        Ok(Self::new(elements(def.elements)?)?)
    }
}

#[derive(Deserialize)]
struct ConfigSchemaDef {
    sections: Vec<ConfigSectionDef>,
}

#[derive(Deserialize)]
struct ConfigSectionDef {
    name: String,
    elements: Vec<FormElementDef>,
}

impl TryFrom<ConfigSchemaDef> for ConfigSchema {
    type Error = Error;

    fn try_from(def: ConfigSchemaDef) -> Result<Self, Self::Error> {
        let sections = def
            .sections
            .into_iter()
            .map(|section| {
                Ok(ConfigSection {
                    name: section.name,
                    elements: elements(section.elements)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self::new(sections)?)
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FormElementDef {
    Text(TextFieldDef),
    Combobox(ComboboxFieldDef),
    Hbox { elements: Vec<FormElementDef> },
}

fn elements(defs: Vec<FormElementDef>) -> Result<Vec<FormElement>, Error> {
    defs.into_iter().map(FormElement::try_from).collect()
}

impl TryFrom<FormElementDef> for FormElement {
    type Error = Error;

    fn try_from(def: FormElementDef) -> Result<Self, Self::Error> {
        Ok(match def {
            FormElementDef::Text(field) => Self::Text(field.into()),
            FormElementDef::Combobox(field) => Self::Combobox(field.try_into()?),
            FormElementDef::Hbox { elements: defs } => Self::Hbox {
                elements: elements(defs)?,
            },
        })
    }
}

#[derive(Deserialize)]
struct TextFieldDef {
    id: String,
    label: String,
    description: Option<String>,
    placeholder: Option<String>,
    #[serde(default)]
    required: bool,
    default: Option<String>,
}

impl From<TextFieldDef> for TextField {
    fn from(def: TextFieldDef) -> Self {
        Self {
            id: def.id,
            label: def.label,
            description: def.description,
            placeholder: def.placeholder,
            required: def.required,
            default: def.default,
        }
    }
}

#[derive(Deserialize)]
struct ComboboxFieldDef {
    id: String,
    label: String,
    description: Option<String>,
    placeholder: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    multiple: bool,
    options: ComboboxOptionsDef,
    #[serde(default)]
    default: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged, expecting = "a list of options or `dynamic`")]
enum ComboboxOptionsDef {
    Static(Vec<ComboboxOptionDef>),
    Dynamic(DynamicKeyword),
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum DynamicKeyword {
    Dynamic,
}

#[derive(Deserialize)]
struct ComboboxOptionDef {
    value: String,
    label: String,
}

impl From<ComboboxOptionsDef> for ComboboxOptions {
    fn from(def: ComboboxOptionsDef) -> Self {
        match def {
            ComboboxOptionsDef::Static(options) => Self::Static(
                options
                    .into_iter()
                    .map(|option| ComboboxOption {
                        value: option.value,
                        label: option.label,
                    })
                    .collect(),
            ),
            ComboboxOptionsDef::Dynamic(DynamicKeyword::Dynamic) => Self::Dynamic,
        }
    }
}

impl TryFrom<ComboboxFieldDef> for ComboboxField {
    type Error = Error;

    fn try_from(def: ComboboxFieldDef) -> Result<Self, Self::Error> {
        let options = def.options.into();
        let mut field = match def.multiple {
            true => Self::multiple(def.id, def.label, options, def.default)?,
            false => {
                let mut defaults = def.default.into_iter();
                let default = defaults.next();
                ensure!(
                    defaults.next().is_none(),
                    "`{}` allows a single value but has many defaults",
                    def.id
                );
                Self::single(def.id, def.label, options, default)?
            }
        };
        field.description = def.description;
        field.placeholder = def.placeholder;
        field.required = def.required;
        Ok(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::component::{Field, FieldValue, Form};
    use std::collections::HashMap;

    #[derive(Deserialize, Debug)]
    struct Manifest {
        #[serde(default, deserialize_with = "login_form")]
        auth: Option<LoginForm>,
        #[serde(default, deserialize_with = "config_schema")]
        config: Option<ConfigSchema>,
    }

    fn parse(yaml: &str) -> Result<Manifest, serde_saphyr::Error> {
        serde_saphyr::from_str(yaml)
    }

    #[track_caller]
    fn check_error(yaml: &str, message: &str) {
        let error = parse(yaml).unwrap_err().to_string();
        assert!(error.contains(message), "expected {message:?} in {error}");
    }

    #[test]
    fn forms_are_optional() {
        let manifest = parse("{}").unwrap();
        assert!(manifest.auth.is_none() && manifest.config.is_none());
    }

    #[test]
    fn forms_are_parsed_into_domain_types() {
        let manifest = parse(
            "
            auth:
              elements:
                - { type: text, id: user, label: User, required: true }
            config:
              sections:
                - name: Library
                  elements:
                    - type: hbox
                      elements:
                        - { type: text, id: region, label: Region, default: eu }
                        - type: combobox
                          id: kinds
                          label: Kinds
                          multiple: true
                          default: [a]
                          options: [{ value: a, label: A }, { value: b, label: B }]
                    - { type: combobox, id: tags, label: Tags, options: dynamic }
            ",
        )
        .unwrap();

        let auth = manifest.auth.unwrap();
        assert!(auth.field("user").unwrap().required());

        let config = manifest.config.unwrap();
        assert_eq!(config.sections()[0].name, "Library");
        let Some(Field::Combobox(tags)) = config.field("tags") else {
            panic!("`tags` should be a combobox");
        };
        assert!(tags.is_dynamic());
        assert_eq!(
            config.resolve(HashMap::new()),
            HashMap::from([
                ("region".to_owned(), FieldValue::Text("eu".to_owned())),
                ("kinds".to_owned(), FieldValue::List(vec!["a".to_owned()])),
            ])
        );
    }

    #[test]
    fn invalid_forms_are_rejected() {
        check_error(
            "{auth: {elements: [{type: combobox, id: k, label: K, \
             options: [{value: a, label: A}, {value: a, label: B}]}]}}",
            "`k` has duplicate option `a`",
        );
        check_error(
            "{config: {sections: [\
             {name: A, elements: [{type: text, id: k, label: K}]}, \
             {name: B, elements: [{type: text, id: k, label: K}]}]}}",
            "duplicate field `k`",
        );
        check_error(
            "{auth: {elements: [{type: combobox, id: k, label: K, default: [a, b], \
             options: [{value: a, label: A}, {value: b, label: B}]}]}}",
            "`k` allows a single value but has many defaults",
        );
        check_error(
            "{auth: {elements: [{type: combobox, id: k, label: K, options: other}]}}",
            "a list of options or `dynamic`",
        );
        check_error(
            "{auth: {elements: [{type: combobox, id: k, label: K, default: [a],              options: dynamic}]}}",
            "`k` cannot have defaults when its options are dynamic",
        );
        check_error(
            "{auth: {elements: [{type: combobox, id: k, label: K, options: dynamic}]}}",
            "field `k` cannot have dynamic options",
        );
    }
}
