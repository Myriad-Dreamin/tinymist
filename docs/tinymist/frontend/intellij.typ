#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: [IntelliJ IDEA])

A comprehensive IntelliJ IDEA plugin for Typst. The plugin provides rich language support and productivity features for Typst documents in IntelliJ IDEA and other JetBrains IDEs.

== Installation

=== From JetBrains Marketplace
+ Open IntelliJ IDEA
+ Go to File → Settings → Plugins (or IntelliJ IDEA → Preferences → Plugins on macOS)
+ Search for "Tinymist"
+ Click Install and restart the IDE

=== Manual Installation
+ Download the latest release from the #link("https://github.com/Myriad-Dreamin/tinymist/releases")[releases page]
+ Go to File #arrow Settings #arrow Plugins
+ Click the gear icon and select "Install Plugin from Disk"
+ Select the downloaded `.zip` file
+ Restart the IDE

== Getting Started

+ Create a new `.typ` file or open an existing one
+ The plugin will automatically activate and provide language support
+ Start typing Typst markup - you'll see syntax highlighting and code completion
+ Use the preview feature to see your document rendered in real-time

== Configuration

=== Custom Tinymist executable
  - Go to File #arrow Settings #arrow Tools #arrow Tinymist LSP
  - Select "Use custom Tinymist executable"
  - Specify the path to your custom `tinymist` executable if needed