import globals from "globals";
import typescriptEslint from "@typescript-eslint/eslint-plugin";
import tsParser from "@typescript-eslint/parser";
import path from "node:path";
import js from "@eslint/js";
import { FlatCompat } from "@eslint/eslintrc";

const __dirname = path.dirname(import.meta.dirname);
const compat = new FlatCompat({
  baseDirectory: __dirname,
  recommendedConfig: js.configs.recommended,
  allConfig: js.configs.all,
});

export default [
  {
    ignores: ["**/.eslintrc.js", "**/out/", "**/node_modules/"],
  },
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
  ...compat
    .extends(
      "eslint:recommended",
      "plugin:@typescript-eslint/eslint-recommended",
      "plugin:@typescript-eslint/recommended",
      "plugin:@typescript-eslint/recommended-requiring-type-checking",
    )
    .map((config) => ({
      ...config,
      files: ["editors/vscode/**/*.ts"],
    })),
  {
    files: ["editors/vscode/**/*.ts"],

    plugins: {
      "@typescript-eslint": typescriptEslint,
    },

    languageOptions: {
      parser: tsParser,
      ecmaVersion: 10,
      sourceType: "module",

      parserOptions: {
        project: true,

        ecmaFeatures: {
          modules: true,
        },
      },
    },

    rules: {
      // "@typescript-eslint/member-delimiter-style": [
      //   "error",
      //   {
      //     multiline: {
      //       delimiter: "semi",
      //       requireLast: true,
      //     },

      //     singleline: {
      //       delimiter: "semi",
      //       requireLast: false,
      //     },
      //   },
      // ],

      semi: [2, "always"],

      "@typescript-eslint/no-inferrable-types": [
        "error",
        {
          ignoreParameters: true,
          ignoreProperties: true,
        },
      ],

      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": [
        "warn", // or "error"
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],

      "require-await": "off",
      "@typescript-eslint/require-await": 0,
      // todo: is it ok to disable this?
      "@typescript-eslint/restrict-template-expressions": 0,
      // todo: enable it in future
      "@typescript-eslint/no-misused-promises": 0,
      "@typescript-eslint/ban-ts-comment": 0,
      "@typescript-eslint/no-empty-function": 0,
      "@typescript-eslint/no-var-requires": 0,
      "@typescript-eslint/no-explicit-any": 0,
      "@typescript-eslint/no-floating-promises": 0,
      "@typescript-eslint/no-unsafe-assignment": 0,
      "@typescript-eslint/no-unsafe-return": 0,
      "@typescript-eslint/no-unsafe-call": 0,
      "@typescript-eslint/no-unsafe-member-access": 0,
      "@typescript-eslint/unbound-method": 0,
    },
  },
];
