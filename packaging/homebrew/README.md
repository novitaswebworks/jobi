# Homebrew tap

`jobi.rb` is the formula. Homebrew installs from a *tap*, a GitHub repository named
`homebrew-<name>`. For `brew install novitaswebworks/tap/jobi` to work:

1. The `jobi` repository must be public (Homebrew clones the tagged source and builds it).
2. Create a public repository `novitaswebworks/homebrew-tap` and copy this file to
   `Formula/jobi.rb` in it.

```sh
gh repo create novitaswebworks/homebrew-tap --public --description "Homebrew tap for Jobi"
git clone https://github.com/novitaswebworks/homebrew-tap && cd homebrew-tap
mkdir -p Formula && cp ../jobi/packaging/homebrew/jobi.rb Formula/jobi.rb
git add Formula && git commit -m "jobi 0.2.0" && git push
```

Users then run:

```sh
brew install novitaswebworks/tap/jobi        # taps automatically, builds from source
brew install --HEAD novitaswebworks/tap/jobi # latest main instead of the tagged release
```

## Releasing a new version

After tagging `vX.Y.Z`, set `tag:` to it and `revision:` to the commit it points at
(`git ls-remote https://github.com/novitaswebworks/jobi vX.Y.Z`), copy the file to the tap, and push.
Check it with `brew install --build-from-source --verbose novitaswebworks/tap/jobi && brew test jobi`.
