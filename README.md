# Translate Tool

[![Actions Status](https://github.com/hamaluik/translatetool/workflows/Rust/badge.svg)](https://github.com/hamaluik/translatetool/actions) [![GitHub license](https://img.shields.io/badge/license-Apache%202-blue.svg)](https://raw.githubusercontent.com/hamaluik/translatetool/master/LICENSE)

Tool for using Google Cloud to automatically translate simple Fluent `.flt` files.

## Compiling

1. Check it out from source
2. Run `cargo build`

## Usage

```
$ tt --help
tt 1.3.0
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
    -f, --from <FILE>           the english translation file to take strings from [default: en.flt]
    -l, --locale <LOCALE>       the locale to translate into ("fr", "it", etc)
    -o, --outpath <PATH>        the path to write the resulting .flt file into [default: .]

SUBCOMMANDS:
    gen-completions    generate shell completions
    help               Prints this message or the help of the given subcommand(s)
    languages          list all possible languages that the template can be translated into
```

### Example:

Translate the following `en.flt` file into French:

```flt
language-name = English

hello-world = Hello, { $who }!

html-test = I am <em>very</em> glad to see you!

range-of-motion-test = Range of Motion Test

shared-photos =
    { $user_name } { $photo_count ->
        [0] hasn't added any photos yet
        [one] added a new photo
       *[other] added { $photo_count } new photos
    }.

liked-comment =
    { $user_name } liked your comment on { $user_gender ->
        [male] his
        [female] her
       *[other] their
    } post.
```

run:

```bash
$ tt -f en.flt -l fr -c gcloud_credentials.json
```

then `fr.flt` will contain:

```flt
language-name = Français

hello-world = Bonjour, { $who }!

html-test = Je suis <em>très</em> content de te voir!

range-of-motion-test = Test d'amplitude de mouvement

shared-photos = { $user_name } ajouté { $___1___ } nouvelles photos.

liked-comment = { $user_name } a aimé votre commentaire sur son post.

```
