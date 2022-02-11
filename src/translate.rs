// Copyright 2020 Kenton Hamaluik
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::error::Error;

use reqwest::blocking::Client;

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryConfig<'a> {
    pub glossary: &'a str,
    pub ignore_case: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslateQuery<'a, 'b, 'c> {
    contents: Vec<&'a str>,
    mime_type: &'static str,
    source_language_code: &'static str,
    target_language_code: &'b str,
    glossary_config: Option<GlossaryConfig<'c>>,
}

#[derive(Deserialize)]
struct TRTranslation {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[derive(Deserialize)]
struct TRData {
    translations: Vec<TRTranslation>,
    #[serde(rename = "glossaryTranslations")]
    glossary_translations: Option<Vec<TRTranslation>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LRLanguage {
    pub language_code: String,
    pub display_name: String,
    pub support_source: bool,
    pub support_target: bool,
}

#[derive(Deserialize)]
struct LRData {
    languages: Vec<LRLanguage>,
}

pub struct Translator<'a, 'b> {
    client: Client,
    token: &'a str,
    project_id: &'a str,
    language: &'b str,
}

impl<'a, 'b> Translator<'a, 'b> {
    pub fn new(token: &'a str, project_id: &'a str, language: &'b str) -> Translator<'a, 'b> {
        Translator {
            client: Client::new(),
            token,
            project_id,
            language,
        }
    }

    pub fn translate<'c>(
        &self,
        phrase: &str,
        glossary: &Option<GlossaryConfig<'c>>,
    ) -> Result<String, Box<dyn Error>> {
        // don't translate en -> en, just copy it over
        if self.language == "en" {
            return Ok(phrase.to_owned());
        }

        let query = TranslateQuery {
            contents: vec![phrase],
            mime_type: "text/html",
            source_language_code: "en",
            target_language_code: self.language,
            glossary_config: glossary.clone(),
        };
        let query = serde_json::to_string(&query)?;

        let res = self
            .client
            .post(&format!(
                "https://translation.googleapis.com/v3/projects/{}/locations/us-central1:translateText",
                self.project_id
            ))
            .bearer_auth(self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(query)
            .send()?;

        if !res.status().is_success() {
            let res = res.text()?;
            eprintln!("query error: {}", res);
            return Err(Box::from(super::errors::Errors::FailedQuery));
        }

        let res = res.text()?;
        let mut res: TRData = serde_json::from_str(&res)?;
        if res.translations.is_empty() {
            return Err(Box::from(super::errors::Errors::NoTranslations));
        }

        let translation = &if let Some(mut glossary_translations) = res.glossary_translations {
            glossary_translations.pop().unwrap().translated_text
        } else {
            res.translations.pop().unwrap().translated_text
        };
        let translation = escaper::decode_html(translation)
            .map_err(|e| format!("failed to decode HTML entities: {:?}", e))?;

        Ok(translation.replace("\n", "\n    ").replace(" ", " "))
    }

    fn get_languages_response(&self) -> Result<LRData, Box<dyn Error>> {
        let res = self
            .client
            .get(&format!("https://translation.googleapis.com/v3/projects/{}/locations/us-central1/supportedLanguages?displayLanguageCode={}", self.project_id, self.language))
            .bearer_auth(self.token)
            .send()?;

        if !res.status().is_success() {
            let res = res.text()?;
            eprintln!("query error: {}", res);
            return Err(Box::from(super::errors::Errors::FailedQuery));
        }

        let res = res.text()?;
        serde_json::from_str(&res).map_err(|err| {
            eprintln!("failed to parse response as json: {:?}", err);
            eprintln!("response was:");
            eprintln!("{}", res);
            Box::from(err)
        })
    }

    pub fn available_languages(&self) -> Result<Vec<LRLanguage>, Box<dyn Error>> {
        let res = self.get_languages_response().map_err(|e| {
            eprintln!("failed to query languages: {:?}", e);
            e
        })?;
        Ok(res
            .languages
            .into_iter()
            .filter(|lang| lang.support_target)
            .collect())
    }

    pub fn get_lang_name(&self) -> Result<String, Box<dyn Error>> {
        let res = self.get_languages_response()?;
        for lang in res.languages {
            if lang.language_code == self.language {
                return Ok(lang.display_name);
            }
        }
        Ok("<INSERT LANGUAGE NAME HERE>".to_owned())
    }
}
