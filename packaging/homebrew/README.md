# Homebrew tap

`jobi.rb` is the formula. Homebrew installs from a *tap*, a GitHub repository named
`homebrew-<name>`. For `brew install novitaswebworks/tap/jobi` to work:

1. The `jobi` repository must be public (Homebrew downloads the release archives from it).
2. Use the public repository `novitaswebworks/homebrew-tap` and copy this file to
   `Formula/jobi.rb` in it.

```sh
gh repo create novitaswebworks/homebrew-tap --public --description "Homebrew tap for Jobi"
git clone https://github.com/novitaswebworks/homebrew-tap && cd homebrew-tap
mkdir -p Formula && cp ../jobi/packaging/homebrew/jobi.rb Formula/jobi.rb
git add Formula && git commit -m "jobi 0.2.0" && git push
```

Users then run:

```sh
brew install novitaswebworks/tap/jobi        # taps automatically; installs a prebuilt binary where one exists
brew install --HEAD novitaswebworks/tap/jobi # builds latest main from source (needs Rust)
```

## Releasing a new version

After the release workflow has attached the archives for `vX.Y.Z`, replace the version in the
URLs and the SHA-256 values (each archive has a `.sha256` file on the release page; the source
archive hash is `curl -L https://github.com/novitaswebworks/jobi/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256`),
copy the file to the tap, and push. Check with `brew audit --strict --formula novitaswebworks/tap/jobi`,
`brew reinstall novitaswebworks/tap/jobi` and `brew test jobi`.

Since v0.3.0 the release has prebuilt binaries for macOS arm64, macOS x86_64 and Linux x86_64; only Linux arm64
builds from source.
