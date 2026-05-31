use lazy_static::lazy_static;
use std::collections::HashMap;
use yew::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    En,
}

#[derive(Clone, Debug, PartialEq)]
pub struct I18n {
    locale: Locale,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        Self { locale }
    }

    pub fn t(&self, key: &str) -> String {
        catalog_for(self.locale)
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn tf(&self, key: &str, args: &[(&str, String)]) -> String {
        let mut value = self.t(key);
        for (name, replacement) in args {
            value = value.replace(&format!("{{{name}}}"), replacement);
        }
        value
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::En)
    }
}

pub fn catalog_for(locale: Locale) -> &'static HashMap<String, String> {
    match locale {
        Locale::En => &EN,
    }
}

#[hook]
pub fn use_i18n() -> I18n {
    use_context::<I18n>().unwrap_or_default()
}

lazy_static! {
    static ref EN: HashMap<String, String> =
        serde_json::from_str(include_str!("en.json")).expect("valid English locale catalog");
}
