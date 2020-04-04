# Translate Tool

[![Actions Status](https://github.com/hamaluik/translatetool/workflows/Rust/badge.svg)](https://github.com/hamaluik/translatetool/actions) [![GitHub license](https://img.shields.io/badge/license-Apache%202-blue.svg)](https://raw.githubusercontent.com/hamaluik/translatetool/master/LICENSE)

Tool for using Google Cloud to automatically translate simple Fluent `.flt` files.

## Compiling

1. Check it out from source
2. Run `cargo build`

## Usage

```
$ tt --help
tt 2.0.0
Kenton Hamaluik <kenton@rehabtronics.com>


USAGE:
    tt [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --credentials <FILE>    the file containing the credentials for Google Cloud APIs. See
                                https://developers.google.com/accounts/docs/application-default-credentials for more
                                information. [default: credentials.json]
    -d, --diff <FILE>           an optional English translation file to diff the terms from to mimimize re-translations
    -f, --from <FILE>           the English translation file to take strings from [default: en.flt]
    -l, --locale <LOCALE>       the locale to translate into ("fr", "it", etc)
    -o, --outpath <PATH>        the path to write the resulting .flt file into [default: .]

SUBCOMMANDS:
    gen-completions    generate shell completions
    help               Prints this message or the help of the given subcommand(s)
    languages          list all possible languages that the template can be translated into
```

### Example:

Translate the [`en.flt`](en.flt) file into French:

```bash
$ tt -f en.flt -l fr -c gcloud_credentials.json
```

The contents should then match [`fr.flt`](fr.flt).
