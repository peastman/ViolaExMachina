# Viola Ex Machina

Viola Ex Machina is an open source, physically inspired synthesizer for stringed instruments:
violin, viola, cello, and bass, both solo instruments and ensembles.

The sound produced by Viola Ex Machina is completely dry.  It models only the singers, not
the room they are in.  To get a realistic sound, it is essential that you add an appropriate
reverb.

### Installing and Using

Viola Ex Machina can be used as a VST3, CLAP, or AUv2 plugin.  [The Releases page](https://github.com/peastman/ViolaExMachina/releases)
has compiled versions for Windows, Linux, and macOS.  If instead you want to build it from
source, first install the [Rust compiler](https://www.rust-lang.org/).  To build the VST3 and CLAP
plugins, execute the following command from this directory.
w
```
cargo xtask bundle viola_ex_machina --release
```

To build the AUv2 plugin, first build the CLAP plugin then follow the instructions in
the `au` subdirectory.

On macOS, you may find it is necessary to compile the plugin yourself.  By default, Apple
blocks all programs from running unless they are digitally signed by a developer who pays
$99/year for an account, which I choose not to do.  There are workarounds which can allow
the precompiled versions to work, but those workarounds have gotten steadily more difficult
with time.  Compiling it yourself avoids this problem.

For instructions on how to use the plugin, see [the documentation](plugin/src/help.md),
which is also available in the plugin's user interface.
