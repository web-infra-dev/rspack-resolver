module.exports = {
  printWidth: 80,
  useTabs: false,
  tabWidth: 2,
  trailingComma: "none",
  arrowParens: "avoid",
  endOfLine: "lf",
  overrides: [
    {
      files: "*.json",
      options: {
        parser: "json",
        useTabs: false
      }
    },
    {
      files: "*.ts",
      options: {
        parser: "typescript"
      }
    },
    {
      files: "website/**",
      options: {
        printWidth: 80,
        singleQuote: true,
        trailingComma: "all",
        useTabs: false
      }
    }
  ]
};
