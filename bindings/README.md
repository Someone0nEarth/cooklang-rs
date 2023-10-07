
# Bindings

This repository exports UniFFI difined bindings that can be used to call Rust Cooklang parser code from languages other than Rust: Kotlin, Swift, Ruby, Python and [some other languages](https://mozilla.github.io/uniffi-rs/#third-party-foreign-language-bindings).

## UniFFI

[UniFFI](https://mozilla.github.io/uniffi-rs/Overview.html) is a brilliant way to define a cross-language interface and associated tools. Rust compiles a C-compatible library with UniFFI metadata baked. Based on this metadata UniFFI compiler can create snippets of code in foreign language that mirrors exposed Rust API.

This particular library employes new-ish [procedural macroses](https://mozilla.github.io/uniffi-rs/proc_macro/index.html) to define exported methods and data-types.

## Exposed API

This library exports methods:

    parse(input: String) -> CooklangRecipe;
    parse_metadata(input: String) -> CooklangMetadata;

    # TODO
    combine_amounts(amounts: Vec<Amount>) -> Vec<Amount>;
    parse_aisle_config(input: String) -> AisleConfig;

### Exposed data structures

    struct CooklangRecipe {
        metadata: CooklangMetadata,
        steps: Vec<Step>,
        ingredients: Vec<Item>,
        cookware: Vec<Item>,
    }

    type CooklangMetadata = HashMap<String, String>;

    struct Step {
        items: Vec<Item>,
    }

    enum Item {
        Text {
            value: String,
        },
        Ingredient {
            name: String,
            amount: Option<Amount>,
        },
        Cookware {
            name: String,
            amount: Option<Amount>,
        },
        Timer {
            name: Option<String>,
            amount: Option<Amount>,
        },
    }

    struct Amount {
        quantity: Value,
        units: Option<String>,
    }

    enum Value {
        Number { value: f64 },
        Range { start: f64, end: f64 },
        Text { value: String },
    }

    struct CombinedIngredient {
        name: String,
        amounts: Vec<Amount>
    }

## Building for Android

### Prepare

Install `rustup` https://www.rust-lang.org/tools/install.

Then add Android targets.

    rustup target add aarch64-linux-android
    rustup target add armv7-linux-androideabi
    rustup target add i686-linux-android
    rustup target add x86_64-linux-android

Install Android NDK https://developer.android.com/studio/projects/install-ndk#default-version.

Add ndk linkers to the PATH variable. Example for ~/.zshrc:

    export PATH=$PATH:/Users/dubadub/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/

### Build

Build library:

    cargo build --lib --target=x86_64-linux-android --release

Biuld foreight language bindings (this will output Kotlin code into `./out` dir:

    cargo run --features="uniffi/cli"  \
      --bin uniffi-bindgen generate \
      --library target/x86_64-linux-android/release/libcooklang.so \
      --language kotlin \
      --out-dir out

See example of a Gradle config [here](https://github.com/cooklang/cooklang-android/blob/main/app/build.gradle#L77-L132) with all required tasks.