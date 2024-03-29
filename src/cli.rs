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

use clap::{App, Arg, SubCommand};

pub fn build_cli() -> App<'static, 'static> {
    App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(Arg::with_name("credentials")
            .short("c")
            .long("credentials")
            .value_name("FILE")
            .takes_value(true)
            .default_value("credentials.json")
            .help("the file containing the credentials for Google Cloud APIs. See https://developers.google.com/accounts/docs/application-default-credentials for more information.")
        )
        .arg(Arg::with_name("from")
            .short("f")
            .long("from")
            .value_name("FILE")
            .takes_value(true)
            .default_value("en.flt")
            .help("the English translation file to take strings from")
        )
        .arg(Arg::with_name("diff")
            .short("d")
            .long("diff")
            .value_name("FILE")
            .takes_value(true)
            .help("an optional English translation file to diff the terms from to mimimize re-translations")
        )
        .arg(Arg::with_name("locale")
            .short("l")
            .long("locale")
            .value_name("LOCALE")
            .takes_value(true)
            .help("the locale to translate into (\"fr\", \"it\", etc)")
        )
        .arg(Arg::with_name("outpath")
            .short("o")
            .long("outpath")
            .value_name("PATH")
            .takes_value(true)
            .default_value(".")
            .help("the path to write the resulting .flt file into")
        )
        .arg(Arg::with_name("glossary")
            .short("g")
            .long("glossary")
            .value_name("GLOSSARY")
            .takes_value(true)
            .help("The glossary name to use (stored in the us-central1 region)")
        )
        .arg(Arg::with_name("ignore-case")
            .long("ignore-case")
            .takes_value(false)
            .help("Ignore case when using a glossary")
        )
        .subcommand(SubCommand::with_name("languages")
            .about("list all possible languages that the template can be translated into")
        )
        .subcommand(SubCommand::with_name("gen-completions")
            .about("generate shell completions")
            .arg(Arg::with_name("shell")
                .required(true)
                .possible_values(&["bash", "zsh", "fish", "powershell", "elvish"])
                .help("the shell to generate completions for")
            )
        )
}
