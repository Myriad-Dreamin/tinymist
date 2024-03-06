module.exports = {
    env: {
        es2021: true,
        node: true,
    },
    extends: [
        "plugin:@typescript-eslint/recommended",
        "plugin:@typescript-eslint/recommended-requiring-type-checking",
        "plugin:@typescript-eslint/strict",
        "prettier",
    ],
    overrides: [],
    parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        project: "tsconfig.json",
    },
    rules: {
        "no-unused-vars": "off",
        "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
        "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
        "class-methods-use-this": "off",
        "@typescript-eslint/explicit-function-return-type": "error",
        "@typescript-eslint/no-explicit-any": "error",
        "init-declarations": "off",
        "@typescript-eslint/init-declarations": "error",
        "no-undef-init": "off",
        "@typescript-eslint/strict-boolean-expressions": [
            "error",
            {
                allowString: false,
                allowNumber: false,
                allowNullableObject: false,
                allowNullableEnum: false,
            },
        ],
    },
};
