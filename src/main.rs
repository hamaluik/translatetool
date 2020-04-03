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
        log::error!("you must provide a credentials files!");
        return Err(Box::from(errors::Errors::MissingCredentialsFile));
    }

    let mut credentials = google_service_credentials::ServiceCredentials::load(credentials_path, "https://www.googleapis.com/auth/cloud-translation")?;
    let token = credentials.get_access_token()?;
    let project_id = credentials.get_project_id();

    Ok((token, project_id))
}

fn continue_parsing<'ast, P: AsRef<Path>>(path: P, r: Result<fluent_syntax::ast::Resource<'ast>, (fluent_syntax::ast::Resource<'ast>, Vec<fluent_syntax::parser::ParserError>)>) -> fluent_syntax::ast::Resource<'ast> {
    match r {
        Ok(r) => r,
        Err((r, errs)) => {
            for err in errs {
                log::warn!("parse error in {}: {:?}", path.as_ref().display(), err);
            }
            r
        }
    }
}

fn find_message<'ast>(resource: &'ast fluent_syntax::ast::Resource<'ast>, id: &str) -> Option<&'ast fluent_syntax::ast::Message<'ast>> {
    for entry in resource.body.iter() {
        if let fluent_syntax::ast::ResourceEntry::Entry(entry) = entry {
            if let fluent_syntax::ast::Entry::Message(message) = &entry {
                if message.id.name == id {
                    return Some(message);
                }
            }
        }
    }
    None
}

fn main() -> Result<(), Box<dyn Error>> {
    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(simplelog::LevelFilter::Debug, simplelog::Config::default(), simplelog::TerminalMode::Mixed).expect("can init termlogger")
    ]).expect("can initiate logging");
    let matches = cli::build_cli().get_matches();

    if let Some(_submatches) = matches.subcommand_matches("languages") {
        let (token, project_id) = get_token_and_project_id(&matches).map_err(|e| {
            log::error!(
                "failed to get token and project id from credentials file: {:?}",
                e
            );
            e
        })?;
        let translator = translate::Translator::new(&token, &project_id, "en");
        let available_languages = translator.available_languages().map_err(|e| {
            log::error!("failed to list available languages from translator!");
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
    let diff_path: Option<PathBuf> = matches.value_of("diff").map(PathBuf::from);
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

    let source = std::fs::read_to_string(from_file)?;
    let source_outdated = if let Some(diff_path) = &diff_path {
        if diff_path.exists() {
            std::fs::read_to_string(diff_path)?
        }
        else {
            String::new()
        }
    }
    else {
        String::new()
    };
    let target_existing = if out_path.exists() {
        std::fs::read_to_string(&out_path)?
    }
    else {
        String::new()
    };

    let source = continue_parsing(&from_file, fluent_syntax::parser::parse(&source));
    let source_outdated = continue_parsing(&diff_path.unwrap_or_default(), fluent_syntax::parser::parse(&source_outdated));
    let target_existing = continue_parsing(&out_path, fluent_syntax::parser::parse(&target_existing));

    let mut translations: HashMap<&str, Option<String>> = HashMap::new();

    for entry in source.body.iter() {
        if let fluent_syntax::ast::ResourceEntry::Entry(entry) = entry {
            if let fluent_syntax::ast::Entry::Message(message) = &entry {
                // check if we need to translate based on diffs
                let needs_translation: bool = if let Some(outdated) = find_message(&source_outdated, message.id.name) {
                    message.value != outdated.value
                }
                else {
                    true
                };

                // disable translation if we have a hand-translated one
                let needs_translation = if let Some(existing) = find_message(&target_existing, message.id.name) {
                    if let Some(comment) = &existing.comment {
                        if let fluent_syntax::ast::Comment::Comment{ content } = comment {
                            !content.iter().any(|c| c.contains("tt-hand-translated"))
                        } else { needs_translation }
                    }
                    else { needs_translation }
                }
                else {
                    needs_translation
                };

                if needs_translation {
                    if let Some(pattern) = &message.value {
                        // prepare the pattern for translating by stripping placeables
                        let source_formatted: String = pattern.elements.iter().map(|pe| match pe {
                            fluent_syntax::ast::PatternElement::TextElement(s) => s,
                            fluent_syntax::ast::PatternElement::Placeable(_) => "___",
                        }).collect();

                        translations.insert(message.id.name, Some(match translator.translate(&source_formatted) {
                            Ok(t) => t,
                            Err(e) => {
                                log::warn!("failed to translate term `{}`: {:?}", message.id.name, e);
                                source_formatted
                            }
                        }));
                    }
                    else {
                        translations.insert(message.id.name, None);
                    }
                }
            }
        }
    }

    for t in translations {
        log::debug!("translated `{}` => `{:?}`", t.0, t.1);
    }

    /*let mut english_source = load_strings(from_file, "en")?;
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
    println!("translations saved to file: {}", out_path.display());*/

    Ok(())
}
