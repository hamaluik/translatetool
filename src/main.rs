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

/// Use the credentials file to sign in to obtain an oauth token for Google translate
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

    let mut credentials = google_service_credentials::ServiceCredentials::load(
        credentials_path,
        "https://www.googleapis.com/auth/cloud-translation",
    )?;
    let token = credentials.get_access_token()?;
    let project_id = credentials.get_project_id();

    Ok((token, project_id))
}

fn continue_parsing<'ast, P: AsRef<Path>>(
    path: P,
    r: Result<
        fluent_syntax::ast::Resource<'ast>,
        (
            fluent_syntax::ast::Resource<'ast>,
            Vec<fluent_syntax::parser::ParserError>,
        ),
    >,
) -> fluent_syntax::ast::Resource<'ast> {
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

fn find_message<'ast>(
    resource: &'ast fluent_syntax::ast::Resource<'ast>,
    id: &str,
) -> Option<&'ast fluent_syntax::ast::Message<'ast>> {
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

fn write_comment<'ast, W: Write>(
    wtr: &mut W,
    comment: Option<&fluent_syntax::ast::Comment<'ast>>,
) -> std::io::Result<()> {
    if let Some(comment) = comment {
        match comment {
            fluent_syntax::ast::Comment::Comment { content } => {
                for c in content {
                    writeln!(wtr, "# {}", c)?;
                }
            }
            fluent_syntax::ast::Comment::GroupComment { content } => {
                for c in content {
                    writeln!(wtr, "## {}", c)?;
                }
            }
            fluent_syntax::ast::Comment::ResourceComment { content } => {
                for c in content {
                    writeln!(wtr, "### {}", c)?;
                }
            }
        }
    }
    Ok(())
}

fn write_expression<'ast, W: Write>(
    wtr: &mut W,
    expression: &fluent_syntax::ast::Expression<'ast>,
) -> std::io::Result<()> {
    match expression {
        fluent_syntax::ast::Expression::InlineExpression(ie) => match ie {
            fluent_syntax::ast::InlineExpression::StringLiteral { value } => {
                write!(wtr, "{{ {} }}", *value)?;
            }
            fluent_syntax::ast::InlineExpression::NumberLiteral { value } => {
                write!(wtr, "{{ {} }}", *value)?;
            }
            fluent_syntax::ast::InlineExpression::FunctionReference { .. } => {
                write!(wtr, "___")?;
            }
            fluent_syntax::ast::InlineExpression::MessageReference { id, .. } => {
                write!(wtr, "{{ {} }}", id.name)?;
            }
            fluent_syntax::ast::InlineExpression::TermReference { id, .. } => {
                write!(wtr, "{{ -{} }}", id.name)?;
            }
            fluent_syntax::ast::InlineExpression::VariableReference { id } => {
                write!(wtr, "{{ ${} }}", id.name)?;
            }
            fluent_syntax::ast::InlineExpression::Placeable { .. } => {
                write!(wtr, "___")?;
            }
        },
        fluent_syntax::ast::Expression::SelectExpression { .. } => {
            write!(wtr, "___")?;
        }
    }
    Ok(())
}

fn write_pattern<'ast, W: Write>(
    wtr: &mut W,
    pattern: &fluent_syntax::ast::Pattern<'ast>,
) -> std::io::Result<()> {
    for element in &pattern.elements {
        match element {
            fluent_syntax::ast::PatternElement::TextElement(s) => {
                wtr.write_all((*s).as_bytes())?;
            }
            fluent_syntax::ast::PatternElement::Placeable(e) => {
                write_expression(wtr, e)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
        simplelog::LevelFilter::Debug,
        simplelog::ConfigBuilder::new()
            .add_filter_allow_str("tt")
            .build(),
        simplelog::TerminalMode::Mixed,
    )
    .expect("can init termlogger")])
    .expect("can initiate logging");
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

    let glossary = matches.value_of("glossary").map(|glossary| {
        format!(
            "projects/{}/locations/us-central1/glossaries/{}",
            project_id, glossary
        )
    });
    let glossary = glossary.as_ref().map(|glossary| translate::GlossaryConfig {
        glossary,
        ignore_case: Some(matches.is_present("ignore-case")),
    });

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
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let target_existing = if out_path.exists() {
        std::fs::read_to_string(&out_path)?
    } else {
        String::new()
    };

    let source = continue_parsing(&from_file, fluent_syntax::parser::parse(&source));
    let source_outdated = continue_parsing(
        &diff_path.unwrap_or_default(),
        fluent_syntax::parser::parse(&source_outdated),
    );
    let target_existing =
        continue_parsing(&out_path, fluent_syntax::parser::parse(&target_existing));

    //let mut translations: HashMap<&str, Option<String>> = HashMap::new();
    let mut pending_translations: HashMap<&str, Option<String>> = HashMap::new();

    for entry in source.body.iter() {
        if let fluent_syntax::ast::ResourceEntry::Entry(entry) = entry {
            if let fluent_syntax::ast::Entry::Message(message) = &entry {
                // check if we need to translate based on diffs
                let needs_translation: bool =
                    if let Some(outdated) = find_message(&source_outdated, message.id.name) {
                        log::debug!("found existing term `{}` in diff", message.id.name);
                        log::debug!("message.value = {:?}", message.value);
                        log::debug!("outdated.value = {:?}", outdated.value);
                        log::debug!(
                            "message.value != outdated.value => {}",
                            message.value != outdated.value
                        );
                        message.value != outdated.value
                    } else {
                        true
                    };
                log::debug!(
                    "term `{}` needs translation from diff: {}",
                    message.id.name,
                    needs_translation
                );

                // disable translation if we have a hand-translated one
                let needs_translation =
                    if let Some(existing) = find_message(&target_existing, message.id.name) {
                        if let Some(comment) = &existing.comment {
                            if let fluent_syntax::ast::Comment::Comment { content } = comment {
                                !content.iter().any(|c| c.contains("tt-hand-translated"))
                            } else {
                                needs_translation
                            }
                        } else {
                            needs_translation
                        }
                    } else {
                        // we always need translation if we don't have the message in the existing file
                        true
                    };
                log::debug!(
                    "term `{}` needs translation after checking hand-translated: {}",
                    message.id.name,
                    needs_translation
                );

                if needs_translation {
                    // deal with language names
                    let is_lang_name = if let Some(comment) = &message.comment {
                        if let fluent_syntax::ast::Comment::Comment { content } = comment {
                            content.iter().any(|c| c.contains("tt-lang-name"))
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_lang_name {
                        pending_translations.insert(
                            message.id.name,
                            Some(match translator.get_lang_name() {
                                Ok(t) => t,
                                Err(e) => {
                                    log::warn!("failed to get language name: {:?}", e);
                                    "<INSERT LANGUAGE NAME HERE>".to_owned()
                                }
                            }),
                        );
                    } else if let Some(pattern) = &message.value {
                        // prepare the pattern for translating by stripping placeables
                        let source_formatted: String = pattern
                            .elements
                            .iter()
                            .map(|pe| match pe {
                                fluent_syntax::ast::PatternElement::TextElement(s) => s,
                                fluent_syntax::ast::PatternElement::Placeable(_) => "___",
                            })
                            .collect();

                        pending_translations.insert(message.id.name, Some(source_formatted));
                    } else {
                        pending_translations.insert(message.id.name, None);
                    }
                }
            }
        }
    }

    log::debug!("pending translations: {:?}", pending_translations);

    let pb = indicatif::ProgressBar::new(pending_translations.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{prefix} {spinner} [{elapsed_precise}] [{wide_bar}] {pos}/{len} ({eta})"),
    );
    pb.set_prefix(locale);

    let translations: HashMap<&str, Option<String>> = pending_translations
        .into_iter()
        .map(|(id, value)| {
            pb.inc(1);
            if let Some(value) = value {
                (
                    id,
                    Some(match translator.translate(&value, &glossary) {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("failed to translate term `{}`: {:?}", id, e);
                            value
                        }
                    }),
                )
            } else {
                (id, None)
            }
        })
        .collect();
    pb.finish();

    // now we have all the translations we need, time to reconstruct a translated .flt file
    let f = fs::File::create(&out_path)?;
    let mut file = BufWriter::new(&f);

    for entry in source.body.iter() {
        if let fluent_syntax::ast::ResourceEntry::Entry(entry) = entry {
            match entry {
                fluent_syntax::ast::Entry::Term(t) => {
                    write_comment(&mut file, t.comment.as_ref())?;
                    write!(&mut file, "-{} = ", t.id.name)?;
                    write_pattern(&mut file, &t.value)?;
                    // TODO: write attributes
                    writeln!(&mut file, "")?;
                    writeln!(&mut file, "")?;
                }
                fluent_syntax::ast::Entry::Message(m) => {
                    // see if we have a new translation for the message
                    if translations.contains_key(m.id.name) {
                        if let Some(msg) = translations.get(m.id.name).unwrap() {
                            // convert each of the placeables
                            let placeables: Vec<String> = if let Some(v) = &m.value {
                                v.elements
                                    .iter()
                                    .filter_map(|e| match e {
                                        fluent_syntax::ast::PatternElement::Placeable(e) => {
                                            let mut text: Vec<u8> = Vec::default();
                                            write_expression(&mut text, e)
                                                .expect("can write_expression on placeable");
                                            let text =
                                                String::from_utf8(text).expect("valid utf-8");
                                            Some(text)
                                        }
                                        _ => None,
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            let mut msg: String = msg.clone();
                            for placeable in placeables.into_iter() {
                                msg = msg.replacen("___", &placeable, 1);
                            }
                            write!(&mut file, "{} = ", m.id.name)?;
                            file.write_all(msg.as_bytes())?;
                            // TODO: write attributes
                        }
                    }
                    // see if there's already a hand-translated message
                    else {
                        // TODO: fix the hand-translated comments
                        log::debug!("checking hand-translated for {}", m.id.name);
                        let message = if let Some(existing) =
                            find_message(&target_existing, m.id.name)
                        {
                            log::debug!("found message in existing");
                            let hand_translated = if let Some(comment) = &existing.comment {
                                if let fluent_syntax::ast::Comment::Comment { content } = comment {
                                    content.iter().any(|c| c.contains("tt-hand-translated"))
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            log::debug!("hand-translated: {}", hand_translated);
                            existing
                        } else {
                            m
                        };

                        write_comment(&mut file, message.comment.as_ref())?;
                        write!(&mut file, "{} = ", m.id.name)?;
                        if let Some(value) = &message.value {
                            write_pattern(&mut file, value)?;
                        }
                        // TODO: write attributes
                    }

                    writeln!(&mut file, "")?;
                    writeln!(&mut file, "")?;
                }
                fluent_syntax::ast::Entry::Comment(c) => {
                    write_comment(&mut file, Some(c))?;
                    writeln!(&mut file, "")?;
                }
            }
        }
    }

    Ok(())
}
