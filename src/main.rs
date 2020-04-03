use fluent::{FluentBundle, FluentResource};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::io::prelude::*;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

mod cli;
mod errors;
mod google_service_credentials;
mod translate;

fn get_token_and_project_id(
    matches: &clap::ArgMatches,
) -> Result<(String, String), Box<dyn Error>> {
    // make sure the credentials file exists
    let credentials_file = matches.value_of("credentials").unwrap();
    let credentials_path = PathBuf::from(credentials_file);
    if !credentials_path.exists() {
        eprintln!("you must provide a credentials files!");
        return Err(Box::from(errors::Errors::MissingCredentialsFile));
    }

    let mut credentials = google_service_credentials::ServiceCredentials::load(credentials_path, "https://www.googleapis.com/auth/cloud-translation")?;
    let token = credentials.get_access_token()?;
    let project_id = credentials.get_project_id();

    Ok((token, project_id))
}

fn load_strings<P: AsRef<Path>, S: ToString>(source: P, locale: S) -> Result<HashMap<String, (String, Vec<String>)>, Box<dyn Error>> {
    let source_contents = fs::read_to_string(source)?;
    let resource =
        FluentResource::try_new(source_contents).map_err(|_| errors::Errors::CantParseResource)?;
    let mut bundle = FluentBundle::new(&[locale]);
    bundle
        .add_resource(&resource)
        .map_err(|_| errors::Errors::CantParseResource)?;

    let mut strings: HashMap<String, (String, Vec<String>)> = HashMap::with_capacity(bundle.entries.len());
    for (term, entry) in bundle.entries.iter() {
        // extract all the variables from this message
        let mut variables: Vec<String> = Vec::new();
        if let fluent_bundle::entry::Entry::Message(msg) = entry {
            if let Some(pattern) = &msg.value {
                for element in pattern.elements.iter() {
                    if let fluent_syntax::ast::PatternElement::Placeable(expr) = element {
                        if let fluent_syntax::ast::Expression::InlineExpression(iexpr) = expr {
                            if let fluent_syntax::ast::InlineExpression::VariableReference {
                                id,
                            } = iexpr
                            {
                                variables.push(id.name.to_owned());
                            } else {
                                variables.push(format!("unparsable-var-{}", variables.len()));
                            }
                        } else {
                            variables.push(format!("unparsable-var-{}", variables.len()));
                        }
                    }
                }
            }
        }

        // get the text of the string
        let result = bundle.format(term, None);
        if result.is_none() {
            return Err(Box::from("Only messages can be parsed at this moment, not terms or functions! Sorry!"));
        }
        let (text, errs) = result.unwrap();
        for err in errs {
            eprintln!("Error formatting string: {}", err);
        }

        // save it
        strings.insert(term.to_owned(), (text, variables));
    }

    Ok(strings)
}

fn emplace_vars(mut text: String, vars: Vec<String>) -> String {
    for var in vars.into_iter() {
        text = text.replacen("___", &format!("{{ ${} }}", var), 1);
    }
    text
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = cli::build_cli().get_matches();

    if let Some(_submatches) = matches.subcommand_matches("languages") {
        let (token, project_id) = get_token_and_project_id(&matches).map_err(|e| {
            eprintln!(
                "failed to get token and project id from credentials file: {:?}",
                e
            );
            e
        })?;
        let translator = translate::Translator::new(&token, &project_id, "en");
        let available_languages = translator.available_languages().map_err(|e| {
            eprintln!("failed to list available languages from translator!");
            e
        })?;

        let available_languages: Vec<String> = available_languages
            .into_iter()
            .map(|lang| format!("{} => '{}'", lang.display_name, lang.language_code))
            .collect();

        println!("Accepted languages:");
        println!("{}", available_languages.join("\n"));
        return Ok(());
    } else if let Some(submatches) = matches.subcommand_matches("gen-completions") {
        let shell = submatches.value_of("shell").unwrap_or("bash");

        cli::build_cli().gen_completions_to(
            env!("CARGO_PKG_NAME"),
            match shell {
                "bash" => clap::Shell::Bash,
                "zsh" => clap::Shell::Zsh,
                "fish" => clap::Shell::Fish,
                "powershell" => clap::Shell::PowerShell,
                "elvish" => clap::Shell::Elvish,
                _ => return Err(Box::from(errors::Errors::InvalidShell)),
            },
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    let (token, project_id) = get_token_and_project_id(&matches)?;
    let from_file = matches.value_of("from").unwrap();
    let diff_path: Option<&str> = matches.value_of("diff");
    let locale = matches
        .value_of("locale")
        .ok_or(errors::Errors::MissingLanguage)?;
    let out_path = Path::new(matches.value_of("outpath").unwrap());
    fs::create_dir_all(out_path)?;
    let out_path = out_path.join(format!("{}.flt", locale));

    let translator = translate::Translator::new(&token, &project_id, locale);
    let available_languages = translator.available_languages()?;
    available_languages
        .iter()
        .find(|lang| lang.language_code == locale)
        .ok_or(errors::Errors::InvalidLanguage)?;

    let mut english_source = load_strings(from_file, "en")?;
    let old_english_source: HashMap<String, String> =
        if out_path.exists() {
            if let Some(diff_path) = diff_path {
                load_strings(diff_path, "en")?
                    .drain()
                    .map(|(term, (text, vars))| (term, emplace_vars(text, vars)))
                    .collect()
            }
            else {
                HashMap::new()
            }
        }
        else {
            HashMap::new()
        };
    let old_translations: HashMap<String, String> =
        if out_path.exists() {
            load_strings(&out_path, locale)?
                .drain()
                .map(|(term, (text, vars))| (term, emplace_vars(text, vars)))
                .collect()
        }
        else {
            HashMap::new()
        };

    let max_key_width = english_source.keys().map(|k| k.len()).max().unwrap();
    let pb = indicatif::ProgressBar::new(english_source
        .iter()
        .filter(|(k, v)| {
            let term: &str = *k;
            let (text, vars): &(String, Vec<String>) = v;

            if let Some(old_text) = old_english_source.get(term) {
                // the term did exist, but the text has been updated, so we need
                // to re-translate
                let text = emplace_vars(text.clone(), vars.clone());
                &text != old_text
            }
            else {
                // the term did _not_ exist, so we need a translations
                true
            }
        })
        .count() as u64);
    pb.set_style(indicatif::ProgressStyle::default_bar().template(&format!(
        "{{spinner}} [{{elapsed_precise}}] [{{wide_bar}}] {{prefix:{}}} {{pos}}/{{len}} ({{eta}})",
        max_key_width
    )));
    let new_translations: Result<HashMap<String, String>, Box<dyn Error>> = english_source
        .drain()
        .filter(|(k, v)| {
            let term: &str = k;
            let (text, vars): &(String, Vec<String>) = v;

            if let Some(old_text) = old_english_source.get(term) {
                // the term did exist, but the text has been updated, so we need
                // to re-translate
                let text = emplace_vars(text.clone(), vars.clone());
                &text != old_text
            }
            else {
                // the term did _not_ exist, so we need a translations
                true
            }
        })
        .enumerate()
        .map(|(i, (term, (text, variables)))| {
            pb.set_position(i as u64);
            pb.set_prefix(&term);

            let mut translation: String =
                if term == "language-name" && text == "English" {
                    translator.translate("<lang name>")?
                } else {
                    translator.translate(&text)?
                };

            // reapply our variables
            translation = emplace_vars(translation, variables);

            Ok((term.to_owned(), translation))
        })
        .collect();
    let mut new_translations = new_translations?;
    pb.finish_with_message("translation complete!");

    // now merge the existing translations together with the new ones
    let mut translations = old_translations;
    for (term, translation) in new_translations.drain() {
        translations.insert(term, translation);
    }

    // sort the terms alphabetically
    let mut translations: Vec<(String, String)> = translations.into_iter().collect();
    translations.sort_by(|a, b| a.0.cmp(&b.0));

    let f = fs::File::create(&out_path)?;
    let mut file = BufWriter::new(&f);

    for (term, translation) in translations.iter() {
        let message = mung::decode_entities(translation);
        file.write_fmt(format_args!("{} = {}\n\n", term, message))?;
    }
    println!("translations saved to file: {}", out_path.display());

    Ok(())
}
