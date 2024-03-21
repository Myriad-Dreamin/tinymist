// todo: these patterns may affect outer scope which is quite bad
//   fenced_code_block_typst:
//   begin: '(`{3,})\s*(?i:(typ|typst)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.typst
//   patterns:
//     - include: source.typst
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_css:
//   begin: '(`{3,})\s*(?i:(css|css.erb)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.css
//   patterns:
//     - include: source.css
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_basic:
//   begin: '(`{3,})\s*(?i:(html|htm|shtml|xhtml|inc|tmpl|tpl)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.html
//   patterns:
//     - include: text.html.basic
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_ini:
//   begin: '(`{3,})\s*(?i:(ini|conf)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.ini
//   patterns:
//     - include: source.ini
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_java:
//   begin: '(`{3,})\s*(?i:(java|bsh)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.java
//   patterns:
//     - include: source.java
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_lua:
//   begin: '(`{3,})\s*(?i:(lua)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.lua
//   patterns:
//     - include: source.lua
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_makefile:
//   begin: '(`{3,})\s*(?i:(Makefile|makefile|GNUmakefile|OCamlMakefile)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.makefile
//   patterns:
//     - include: source.makefile
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_perl:
//   begin: '(`{3,})\s*(?i:(perl|pl|pm|pod|t|PL|psgi|vcl)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.perl
//   patterns:
//     - include: source.perl
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_r:
//   begin: '(`{3,})\s*(?i:(R|r|s|S|Rprofile|\{\.r.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.r
//   patterns:
//     - include: source.r
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_ruby:
//   begin: '(`{3,})\s*(?i:(ruby|rb|rbx|rjs|Rakefile|rake|cgi|fcgi|gemspec|irbrc|Capfile|ru|prawn|Cheffile|Gemfile|Guardfile|Hobofile|Vagrantfile|Appraisals|Rantfile|Berksfile|Berksfile.lock|Thorfile|Puppetfile)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.ruby
//   patterns:
//     - include: source.ruby
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_php:
//   begin: '(`{3,})\s*(?i:(php|php3|php4|php5|phpt|phtml|aw|ctp)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.php
//   patterns:
//     - include: text.html.basic
//     - include: source.php
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_sql:
//   begin: '(`{3,})\s*(?i:(sql|ddl|dml)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.sql
//   patterns:
//     - include: source.sql
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_vs_net:
//   begin: '(`{3,})\s*(?i:(vb)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.vs_net
//   patterns:
//     - include: source.asp.vb.net
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_xml:
//   begin: '(`{3,})\s*(?i:(xml|xsd|tld|jsp|pt|cpt|dtml|rss|opml)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.xml
//   patterns:
//     - include: text.xml
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_xsl:
//   begin: '(`{3,})\s*(?i:(xsl|xslt)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.xsl
//   patterns:
//     - include: text.xml.xsl
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_yaml:
//   begin: '(`{3,})\s*(?i:(yaml|yml)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.yaml
//   patterns:
//     - include: source.yaml
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_dosbatch:
//   begin: '(`{3,})\s*(?i:(bat|batch)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.dosbatch
//   patterns:
//     - include: source.batchfile
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_clojure:
//   begin: '(`{3,})\s*(?i:(clj|cljs|clojure)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.clojure
//   patterns:
//     - include: source.clojure
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_coffee:
//   begin: '(`{3,})\s*(?i:(coffee|Cakefile|coffee.erb)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.coffee
//   patterns:
//     - include: source.coffee
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_c:
//   begin: '(`{3,})\s*(?i:(c|h)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.c
//   patterns:
//     - include: source.c
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_cpp:
//   begin: '(`{3,})\s*(?i:(cpp|c\+\+|cxx)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.cpp source.cpp
//   patterns:
//     - include: source.cpp
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_diff:
//   begin: '(`{3,})\s*(?i:(patch|diff|rej)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.diff
//   patterns:
//     - include: source.diff
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_dockerfile:
//   begin: '(`{3,})\s*(?i:(dockerfile|Dockerfile)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.dockerfile
//   patterns:
//     - include: source.dockerfile
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_git_commit:
//   begin: '(`{3,})\s*(?i:(COMMIT_EDITMSG|MERGE_MSG)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.git_commit
//   patterns:
//     - include: text.git-commit
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_git_rebase:
//   begin: '(`{3,})\s*(?i:(git-rebase-todo)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.git_rebase
//   patterns:
//     - include: text.git-rebase
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_go:
//   begin: '(`{3,})\s*(?i:(go|golang)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.go
//   patterns:
//     - include: source.go
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_groovy:
//   begin: '(`{3,})\s*(?i:(groovy|gvy)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.groovy
//   patterns:
//     - include: source.groovy
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_pug:
//   begin: '(`{3,})\s*(?i:(jade|pug)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.pug
//   patterns:
//     - include: text.pug
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_js:
//   begin: '(`{3,})\s*(?i:(js|jsx|javascript|es6|mjs|cjs|dataviewjs|\{\.js.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.javascript
//   patterns:
//     - include: source.js
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_js_regexp:
//   begin: '(`{3,})\s*(?i:(regexp)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.js_regexp
//   patterns:
//     - include: source.js.regexp
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_json:
//   begin: '(`{3,})\s*(?i:(json|json5|sublime-settings|sublime-menu|sublime-keymap|sublime-mousemap|sublime-theme|sublime-build|sublime-project|sublime-completions)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.json
//   patterns:
//     - include: source.json
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_jsonc:
//   begin: '(`{3,})\s*(?i:(jsonc)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.jsonc
//   patterns:
//     - include: source.json.comments
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_less:
//   begin: '(`{3,})\s*(?i:(less)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.less
//   patterns:
//     - include: source.css.less
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_objc:
//   begin: '(`{3,})\s*(?i:(objectivec|objective-c|mm|objc|obj-c|m|h)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.objc
//   patterns:
//     - include: source.objc
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_swift:
//   begin: '(`{3,})\s*(?i:(swift)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.swift
//   patterns:
//     - include: source.swift
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_scss:
//   begin: '(`{3,})\s*(?i:(scss)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.scss
//   patterns:
//     - include: source.css.scss
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_perl6:
//   begin: '(`{3,})\s*(?i:(perl6|p6|pl6|pm6|nqp)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.perl6
//   patterns:
//     - include: source.perl.6
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_powershell:
//   begin: '(`{3,})\s*(?i:(powershell|ps1|psm1|psd1)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.powershell
//   patterns:
//     - include: source.powershell
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_python:
//   begin: '(`{3,})\s*(?i:(python|py|py3|rpy|pyw|cpy|SConstruct|Sconstruct|sconstruct|SConscript|gyp|gypi|\{\.python.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.python
//   patterns:
//     - include: source.python
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_julia:
//   begin: '(`{3,})\s*(?i:(julia|\{\.julia.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.julia
//   patterns:
//     - include: source.julia
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_regexp_python:
//   begin: '(`{3,})\s*(?i:(re)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.regexp_python
//   patterns:
//     - include: source.regexp.python
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_rust:
//   begin: '(`{3,})\s*(?i:(rust|rs|\{\.rust.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.rust
//   patterns:
//     - include: source.rust
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_scala:
//   begin: '(`{3,})\s*(?i:(scala|sbt)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.scala
//   patterns:
//     - include: source.scala
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_shell:
//   begin: '(`{3,})\s*(?i:(shell|sh|bash|zsh|bashrc|bash_profile|bash_login|profile|bash_logout|.textmate_init|\{\.bash.+?\})\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.shellscript
//   patterns:
//     - include: source.shell
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_ts:
//   begin: '(`{3,})\s*(?i:(typescript|ts)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.typescript
//   patterns:
//     - include: source.ts
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_tsx:
//   begin: '(`{3,})\s*(?i:(tsx)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.typescriptreact
//   patterns:
//     - include: source.tsx
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_csharp:
//   begin: '(`{3,})\s*(?i:(cs|csharp|c#)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.csharp
//   patterns:
//     - include: source.cs
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_fsharp:
//   begin: '(`{3,})\s*(?i:(fs|fsharp|f#)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.fsharp
//   patterns:
//     - include: source.fsharp
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_dart:
//   begin: '(`{3,})\s*(?i:(dart)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.dart
//   patterns:
//     - include: source.dart
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_handlebars:
//   begin: '(`{3,})\s*(?i:(handlebars|hbs)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.handlebars
//   patterns:
//     - include: text.html.handlebars
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_markdown:
//   begin: '(`{3,})\s*(?i:(markdown|md)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.markdown
//   patterns:
//     - include: text.html.markdown
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_log:
//   begin: '(`{3,})\s*(?i:(log)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.log
//   patterns:
//     - include: text.log
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_erlang:
//   begin: '(`{3,})\s*(?i:(erlang)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.erlang
//   patterns:
//     - include: source.erlang
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_elixir:
//   begin: '(`{3,})\s*(?i:(elixir)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.elixir
//   patterns:
//     - include: source.elixir
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_latex:
//   begin: '(`{3,})\s*(?i:(latex|tex)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.latex
//   patterns:
//     - include: text.tex.latex
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_bibtex:
//   begin: '(`{3,})\s*(?i:(bibtex)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.bibtex
//   patterns:
//     - include: text.bibtex
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// fenced_code_block_twig:
//   begin: '(`{3,})\s*(?i:(twig)\b)'
//   # ([\s\S]*)(\1)
//   end: (\1)
//   name: markup.raw.block.typst
//   contentName: meta.embedded.block.twig
//   patterns:
//     - include: source.twig
//   beginCaptures:
//     '1':
//       name: punctuation.definition.raw.begin.typst
//     '2':
//       name: fenced_code.block.language.typst
//   endCaptures:
//     '1':
//       name: punctuation.definition.raw.end.typst
// - include: '#fenced_code_block_typst'
// - include: '#fenced_code_block_css'
// - include: '#fenced_code_block_basic'
// - include: '#fenced_code_block_ini'
// - include: '#fenced_code_block_java'
// - include: '#fenced_code_block_lua'
// - include: '#fenced_code_block_makefile'
// - include: '#fenced_code_block_perl'
// - include: '#fenced_code_block_r'
// - include: '#fenced_code_block_ruby'
// - include: '#fenced_code_block_php'
// - include: '#fenced_code_block_sql'
// - include: '#fenced_code_block_vs_net'
// - include: '#fenced_code_block_xml'
// - include: '#fenced_code_block_xsl'
// - include: '#fenced_code_block_yaml'
// - include: '#fenced_code_block_dosbatch'
// - include: '#fenced_code_block_clojure'
// - include: '#fenced_code_block_coffee'
// - include: '#fenced_code_block_c'
// - include: '#fenced_code_block_cpp'
// - include: '#fenced_code_block_diff'
// - include: '#fenced_code_block_dockerfile'
// - include: '#fenced_code_block_git_commit'
// - include: '#fenced_code_block_git_rebase'
// - include: '#fenced_code_block_go'
// - include: '#fenced_code_block_groovy'
// - include: '#fenced_code_block_pug'
// - include: '#fenced_code_block_js'
// - include: '#fenced_code_block_js_regexp'
// - include: '#fenced_code_block_json'
// - include: '#fenced_code_block_jsonc'
// - include: '#fenced_code_block_less'
// - include: '#fenced_code_block_objc'
// - include: '#fenced_code_block_swift'
// - include: '#fenced_code_block_scss'
// - include: '#fenced_code_block_perl6'
// - include: '#fenced_code_block_powershell'
// - include: '#fenced_code_block_python'
// - include: '#fenced_code_block_julia'
// - include: '#fenced_code_block_regexp_python'
// - include: '#fenced_code_block_rust'
// - include: '#fenced_code_block_scala'
// - include: '#fenced_code_block_shell'
// - include: '#fenced_code_block_ts'
// - include: '#fenced_code_block_tsx'
// - include: '#fenced_code_block_csharp'
// - include: '#fenced_code_block_fsharp'
// - include: '#fenced_code_block_dart'
// - include: '#fenced_code_block_handlebars'
// - include: '#fenced_code_block_markdown'
// - include: '#fenced_code_block_log'
// - include: '#fenced_code_block_erlang'
// - include: '#fenced_code_block_elixir'
// - include: '#fenced_code_block_latex'
// - include: '#fenced_code_block_bibtex'
// - include: '#fenced_code_block_twig'
