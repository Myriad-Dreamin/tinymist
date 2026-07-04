
= System Configuration

Add following content to the end of your `~/.config/nix/nix.conf` file to enable flakes and nix-command experimental-features:

```conf
experimental-features = nix-command flakes
```

To debug bundle size:

```
nix-shell -p nix-tree.out --run nix-tree
```

= Running devShell

Run with built binaries from source:

```
nix develop
```

Run with the binaries from the nixpkgs unstable:

```
nix develop .#unstable
```
