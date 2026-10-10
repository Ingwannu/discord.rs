// Regenerates src/formatters/escape_golden.json from the published
// @discordjs/formatters build:
//   npm pack @discordjs/formatters@0.6.2 && tar xzf discordjs-formatters-0.6.2.tgz
//   node generate.js package/dist/index.js > escape_golden.json
const f = require(require('path').resolve(process.argv[2]));
const inputs = [
  "", "plain text", "**bold**", "*italic*", "_italic_", "__underline__", "***bold italic***",
  "___underline italic___", "****", "*****", "******", "______", "~~strike~~", "~~~", "||spoiler||", "|||",
  "`code`", "``code``", "```code```", "````", "```js\nlet x = 1;\n```", "a\\b", "\\*", "\\\\_x_",
  "snake_case_name", "2 * 3 * 4", "a**b", "**", "*", "_", "__", "___", "***",
  "https://example.com/a_b*c__d", "see https://x.y/_a_ and _b_", "http://a.b/*x* *y*",
  "<:my_emoji:123456789012345678>", "<a:a_b__c:123456789012345678>",
  "foo_:123>", "x__:42>", "a___:7>",
  "# heading", "## h2", "### h3", "#### h4", "#no", "  # indented", "    # deep", "- # bullet heading",
  "* # star heading", "   - # too indented", "-  #  spaced", "- item", "* item", "  - nested", "-no", "- ",
  "1. one", "  10. ten", "1) paren", "12.5 percent",
  "[text](https://example.com)", "[a](b) and more", "[](x)", "[a]()", "[a] (b)",
  "nested **bold _italic_ bold**", "unicode ✨ **볼드** _이탤릭_ 😀 ~~취소~~",
  "mix `code **not bold**` **bold**", "line1\nline2 *a*\n- item\n# head\n1. x\n[l](u)",
  "\r\n- crlf item\r\n# h", "tab\t*x*", "a_b_c_d", "_a_b_", "**a**b**", "*a**b*", "__a__b__",
  "`a``b```c````d", "x ``` y ``` z", "x ``` y", "`a` b `c", "``a` b`", "**`x`**", "~~||x||~~",
  "*_*_", "_*_*", "**__**__", "a - item # h", "  * # x", "*  ##  y",
];
const options = [
  {},
  { heading: true, bulletedList: true, numberedList: true, maskedLink: true },
  { bold: false }, { italic: false }, { underline: false }, { bold: false, italic: false },
  { codeBlockContent: false }, { codeBlockContent: false, codeBlock: false },
  { inlineCodeContent: false }, { inlineCodeContent: false, inlineCode: false },
  { codeBlockContent: false, inlineCodeContent: false },
  { escape: false }, { codeBlock: false }, { inlineCode: false },
  { strikethrough: false, spoiler: false },
  { heading: true }, { bulletedList: true }, { numberedList: true }, { maskedLink: true },
];
const escapers = ['escapeCodeBlock', 'escapeInlineCode', 'escapeItalic', 'escapeBold', 'escapeUnderline',
  'escapeStrikethrough', 'escapeSpoiler', 'escapeEscape', 'escapeHeading', 'escapeBulletedList',
  'escapeNumberedList', 'escapeMaskedLink'];
const out = {
  source: '@discordjs/formatters 0.6.2 (dist/index.js)',
  options,
  inputs,
  escape_markdown: inputs.map((input) => options.map((o) => f.escapeMarkdown(input, o))),
  escapers: Object.fromEntries(escapers.map((name) => [name, inputs.map((input) => f[name](input))])),
};
process.stdout.write(JSON.stringify(out) + '\n');
