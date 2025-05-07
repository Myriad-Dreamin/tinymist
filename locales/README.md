
## Locales

This folder provides the locale files for the applications. The format of the locale files is TOML. It looks like this:

```
[extension.tinymist.command.tinymist.pinMainToCurrent] # K
en = "Pin the Main file to the Opening Document" # V-en
zh-CN = "将主文件固定到当前打开的文档" # zh-CN
```

Explanation:

- The key `K` is the key of the locale string. It is a string that is used to identify the locale string.
  - The key is `extension.tinymist.command.tinymist.pinMainToCurrent` in the example, wrapped in square brackets.
- The value `V-en` is the English locale string, which is also the first locale string used in the application.
    - The value is `"Pin the Main file to the Opening Document"` in the example.
- The value `V-zh-CN` is the Chinese locale string. It is used when the locale of the application is set to Chinese. The rest locale strings are sorted by alphabet order.
    - The value is `"将主文件固定到当前打开的文档"` in the example.

The format is designed to be easy to modified by LLM.
