import * as textmate from "./textmate.mjs";

const IDENTIFIER_BARE = /[\p{XID_Start}_][\p{XID_Continue}_\-]*/u;

const blockRawLangGen =
  (ass0: string | undefined, ...ass: string[]) =>
  (...candidates: string[]): textmate.Pattern => {
    const lang = candidates[0];
    const sourcePatterns = [
      {
        include: ass0 || `source.${lang}`,
      },
      ...ass.map((include) => ({ include })),
    ];

    const enter = (n: number): textmate.Pattern => ({
      begin: new RegExp(
        "(`{" + n.toString() + "})" + `(${candidates.join("|")})\\b`
      ),
      beginCaptures: {
        "1": {
          name: "punctuation.definition.raw.begin.typst",
        },
        "2": {
          name: "fenced_code.block.language.typst",
        },
      },
      end: /(\1)/,
      endCaptures: {
        "1": {
          name: "punctuation.definition.raw.end.typst",
        },
      },
      patterns: [
        {
          begin: /(^|\G)(\s*)/,
          // end: "(?=`{" + n.toString() + ",})",
          while: "(^|\\G)(?!`{" + n.toString() + ",})",
          contentName: `meta.embedded.block.${lang}`,
          patterns: sourcePatterns,
        },
      ],
    });

    return {
      name: `markup.raw.block.${lang}`,
      patterns: [
        // one line case
        {
          match: new RegExp(
            /(`{3,})/.source +
              `(${candidates.join("|")})` +
              /\b(.*?)(\1)/.source
          ),
          captures: {
            "1": {
              name: "punctuation.definition.raw.begin.typst",
            },
            "2": {
              name: "fenced_code.block.language.typst",
            },
            "3": {
              name: `meta.embedded.block.${lang}`,
              patterns: sourcePatterns,
            },
            "4": {
              name: "punctuation.definition.raw.end.typst",
            },
          },
        },
        ...[6, 5, 4, 3].map(enter),
      ],
    };
  };

const blockRawLangAs = (as?: string) => blockRawLangGen(as);
const blockRawLang = blockRawLangAs();

const ENABLE_RAW_RENDERING = true;

const blockRawLangs_ = [
  blockRawLang("typst", "typ"),
  blockRawLang("typst-code", "typc"),
  blockRawLang("css", "css.erb"),
  blockRawLangAs("text.html.basic")(
    "html",
    "htm",
    "shtml",
    "xhtml",
    "inc",
    "tmpl",
    "tpl"
  ),
  blockRawLang("ini", "conf"),
  blockRawLang("java", "bsh"),
  blockRawLang("lua"),
  blockRawLang("makefile", "makefile", "GNUmakefile", "OCamlMakefile"),
  blockRawLang("perl", "pl", "pm", "pod", "t", "PL", "psgi", "vcl"),
  blockRawLang("r", "R", "r", "s", "S", "Rprofile"),
  blockRawLang(
    "ruby",
    "rb",
    "rbx",
    "rjs",
    "Rakefile",
    "rake",
    "cgi",
    "fcgi",
    "gemspec",
    "irbrc",
    "Capfile",
    "ru",
    "prawn",
    "Cheffile",
    "Gemfile",
    "Guardfile",
    "Hobofile",
    "Vagrantfile",
    "Appraisals",
    "Rantfile",
    "Berksfile",
    "Berksfile.lock",
    "Thorfile",
    "Puppetfile"
  ),
  blockRawLangGen("text.html.basic", "source.php")(
    "php",
    "php",
    "php3",
    "php4",
    "php5",
    "phpt",
    "phtml",
    "aw",
    "ctp"
  ),
  blockRawLang("sql", "ddl", "dml"),
  blockRawLangAs("source.asp.vb.net")("vb"),
  blockRawLangAs("text.xml")(
    "xml",
    "xsd",
    "tld",
    "jsp",
    "pt",
    "cpt",
    "dtml",
    "rss",
    "opml"
  ),
  blockRawLangAs("text.xml.xsl")("xsl", "xslt"),
  blockRawLang("yaml", "yml"),
  blockRawLang("batchfile", "bat", "batch"),
  blockRawLang("clojure", "clj", "cljs"),
  blockRawLang("coffee", "Cakefile", "coffee.erb"),
  blockRawLang("c", "h"),
  blockRawLang("cpp", "c\\+\\+", "cxx"),
  blockRawLang("diff", "patch", "rej"),
  blockRawLang("dockerfile", "Dockerfile"),
  blockRawLangAs("text.git-commit")(
    "git-commit",
    "COMMIT_EDITMSG",
    "MERGE_MSG"
  ),
  blockRawLangAs("text.git-rebase")("git-rebase", "git-rebase-todo"),
  blockRawLang("go", "golang"),
  blockRawLang("groovy", "gvy"),
  blockRawLangAs("text.pug")("pug", "jade"),
  blockRawLang("js", "jsx", "javascript", "es6", "mjs", "cjs", "dataviewjs"),
  blockRawLangAs("source.js.regexp")("regexp"),
  blockRawLang(
    "json",
    "json5",
    "sublime-settings",
    "sublime-menu",
    "sublime-keymap",
    "sublime-mousemap",
    "sublime-theme",
    "sublime-build",
    "sublime-project",
    "sublime-completions"
  ),
  blockRawLangAs("source.json.comments")("jsonc"),
  blockRawLangAs("source.css.less")("less"),
  blockRawLang("objc", "objective-c", "mm", "obj-c", "m", "h"),
  blockRawLang("swift"),
  blockRawLangAs("source.css.scss")("scss"),
  blockRawLangAs("source.perl.6")("perl6", "p6", "pl6", "pm6", "nqp"),
  blockRawLang("powershell", "ps1", "psm1", "psd1"),
  blockRawLang(
    "python",
    "py",
    "py3",
    "rpy",
    "pyw",
    "cpy",
    "SConstruct",
    "Sconstruct",
    "sconstruct",
    "SConscript",
    "gyp",
    "gypi"
  ),
  blockRawLang("julia"),
  blockRawLangAs("source.regexp.python")("re"),
  blockRawLang("rust", "rs"),
  blockRawLang("scala", "sbt"),
  blockRawLang(
    "shell",
    "sh",
    "bash",
    "zsh",
    "bashrc",
    "bash_profile",
    "bash_login",
    "profile",
    "bash_logout",
    ".textmate_init"
  ),
  blockRawLang("ts", "typescript"),
  blockRawLang("tsx"),
  blockRawLang("cs", "csharp", "c#"),
  blockRawLang("fs", "fsharp", "f#"),
  blockRawLang("dart"),
  blockRawLangAs("text.html.handlebars")("handlebars", "hbs"),
  blockRawLangAs("text.html.markdown")("markdown", "md"),
  blockRawLangAs("text.log")("log"),
  blockRawLang("erlang"),
  blockRawLang("elixir"),
  blockRawLangAs("text.tex.latex")("latex", "tex"),
  blockRawLangAs("text.bibtex")("bibtex"),
  blockRawLang("twig"),
];
export const blockRawLangs = ENABLE_RAW_RENDERING ? blockRawLangs_ : [];

export const inlineRaw: textmate.Pattern = {
  name: "markup.raw.inline.typst",
  begin: /`/,
  end: /`/,
  beginCaptures: {
    "0": {
      name: "punctuation.definition.raw.inline.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "punctuation.definition.raw.inline.typst",
    },
  },
};

export const blockRaw: textmate.Pattern = {
  patterns: [
    ...blockRawLangs.map((blockRawLang) => ({
      include: "#" + blockRawLang.name!.replace(/\./g, "_"),
    })),
    {
      include: "#blockRawGeneral",
    },
  ],
};

export const blockRawGeneral: textmate.Pattern = {
  name: "markup.raw.block.typst",
  begin: new RegExp(/(`{3,})/.source + `(${IDENTIFIER_BARE.source}\\b)?`),
  beginCaptures: {
    "1": {
      name: "punctuation.definition.raw.begin.typst",
    },
    "2": {
      name: "fenced_code.block.language.typst",
    },
  },
  end: /(\1)/,
  endCaptures: {
    "1": {
      name: "punctuation.definition.raw.end.typst",
    },
  },
};
