/** The reader's structured comments nest; a bare semicolon elsewhere is text. */
export default {
  name: "parsh",
  displayName: "parsh",
  scopeName: "source.parsh",
  patterns: [
    { include: "#comment" },
    { include: "#string" },
    { include: "#literal" },
    {
      begin: "(\\()(defn|defmacro)\\s+([^\\s()\\[\\]{}\"'$]+)\\s+(\\()",
      beginCaptures: {
        1: { name: "punctuation.section.parens.parsh" },
        2: { name: "keyword.control.parsh" },
        3: { name: "entity.name.function.parsh" },
        4: { name: "punctuation.section.parens.parsh" },
      },
      end: "\\)",
      endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      patterns: [
        { include: "#parameters" },
      ],
    },
    {
      begin: "(\\()(fn)\\s+(\\()",
      beginCaptures: {
        1: { name: "punctuation.section.parens.parsh" },
        2: { name: "keyword.control.parsh" },
        3: { name: "punctuation.section.parens.parsh" },
      },
      end: "\\)",
      endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      patterns: [{ include: "#parameters" }],
    },
    {
      match: "(\\()(let|set|for|export)\\s+([^\\s()\\[\\]{}\"'$]+)",
      captures: {
        1: { name: "punctuation.section.parens.parsh" },
        2: { name: "keyword.control.parsh" },
        3: { name: "variable.other.parsh" },
      },
    },
    { name: "variable.other.parsh", match: "\\$[^\\s()\\[\\]{}\"']+" },
    {
      name: "keyword.control.parsh",
      match:
        "(?<=\\()(?:capture-call|stdout-text|stderr-text|exit-code|job-status|callable\\?|command\\?|bytes\\?|json/encode|json/decode|bytes/len|bytes/slice|bytes/concat|bytes/from-text|bytes/to-text|bytes/base64-encode|bytes/base64-decode|bytes/hex-encode|bytes/hex-decode|http/request|http/get|http/post|http/body-text|finally|command|signal|cancel|spawn|throw|catch|check|wait|try|unquote-splicing|quasiquote|defstruct|str/contains|function\\?|continue|function|defmacro|str/replace|unquote|list/append|list/concat|list/map|list/filter|list/fold|list/any|list/all|list/reverse|list/contains|number\\?|struct\\?|status\\?|return|export|list/first|list/slice|str/slice|str/split|str/lines|typeof|float\\?|empty\\?|while|break|quote|map/keys|map/vals|map/merge|map/entries|map/from-entries|list/rest|str/join|str/trim|text\\?|bool\\?|list\\?|form\\?|floor|round|apply|defn|glob|exit|type|list|map/get|map/set|map/del|map/has|map/len|list/len|list/get|str/len|str/get|int\\?|map\\?|nil\\?|quot|ceil|float|bool|int|let|set|and|for|get|map|str|not|rem|abs|min|max|do|if|or|fn|cd|eq)(?=[\\s()\\[\\]{}\"'])",
    },
    {
      name: "keyword.operator.parsh",
      match: "(?<=\\()(?:[+*/=-]|!=|[<>]=?|\\|[12]?>?>?|<\\|)(?=[\\s()\\[\\]{}\"'])",
    },
    { name: "entity.name.function.parsh", match: "(?<=\\()[^\\s()\\[\\]{}\"'$]+" },
    { name: "punctuation.section.parens.parsh", match: "[()\\[\\]{}]" },
    { name: "punctuation.section.parsh", match: "`" },
  ],
  repository: {
    literal: {
      patterns: [
        {
          name: "constant.language.parsh",
          match: "(?<![^\\s()\\[\\]{}\"'`,])#(?:true|false|nil)(?=[\\s()\\[\\]{}\"',]|$)",
        },
        {
          name: "constant.numeric.parsh",
          match: "(?<![^\\s()\\[\\]{}\"'`,])#[+-]?(?:[0-9]+(?:\\.[0-9]*)?|\\.[0-9]+)(?:[eE][+-]?[0-9]+)?(?=[\\s()\\[\\]{}\"',]|$)",
        },
      ],
    },
    parameters: {
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { name: "keyword.control.parsh", match: "&(?:optional|rest)(?=[\\s()])" },
        {
          begin: "(\\()([^\\s()\\[\\]{}\"'$]+)",
          beginCaptures: {
            1: { name: "punctuation.section.parens.parsh" },
            2: { name: "variable.parameter.parsh" },
          },
          end: "\\)",
          endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
          patterns: [{ include: "$self" }],
        },
        { name: "variable.parameter.parsh", match: "[^\\s()\\[\\]{}\"']+" },
      ],
    },
    string: {
      patterns: [
        { include: "#triple-single-string" },
        { include: "#triple-double-string" },
        { include: "#single-string" },
        { include: "#double-string" },
      ],
    },
    "triple-single-string": {
      name: "string.quoted.single.parsh",
      begin: "'''",
      end: "'''",
    },
    "triple-double-string": {
      name: "string.quoted.double.parsh",
      begin: '"""',
      end: '"""',
      patterns: [{ include: "#double-string-content" }],
    },
    "single-string": {
      name: "string.quoted.single.parsh",
      begin: "'",
      end: "'",
    },
    "double-string": {
      name: "string.quoted.double.parsh",
      begin: '\"',
      end: '\"',
      patterns: [{ include: "#double-string-content" }],
    },
    "double-string-content": {
      patterns: [
        { name: "constant.character.escape.parsh", match: "\\\\." },
        { include: "#interpolation" },
        {
          name: "variable.other.parsh",
          match: "\\$(?:\\?|[\\p{Alphabetic}_][\\p{Alphabetic}\\p{Number}_]*)",
        },
      ],
    },
    interpolation: {
      begin: "\\$\\(",
      end: "\\)",
      beginCaptures: { 0: { name: "punctuation.section.interpolation.begin.parsh" } },
      endCaptures: { 0: { name: "punctuation.section.interpolation.end.parsh" } },
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { include: "#interpolation-list" },
        { include: "$self" },
      ],
    },
    "interpolation-list": {
      begin: "\\(",
      end: "\\)",
      beginCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { include: "#interpolation-list" },
        { include: "$self" },
      ],
    },
    comment: {
      name: "comment.block.parsh",
      begin: "\\(\\s*;(?=[\\s()\\[\\]{}\"']|$)",
      end: "\\)",
      patterns: [{ include: "#string" }, { include: "#comment-body" }],
    },
    "comment-body": {
      begin: "\\(",
      end: "\\)",
      patterns: [{ include: "#string" }, { include: "#comment-body" }],
    },
  },
} as const;
