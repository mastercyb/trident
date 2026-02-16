; Keywords
[
  "program"
  "module"
  "use"
  "fn"
  "pub"
  "sec"
  "let"
  "mut"
  "const"
  "struct"
  "if"
  "else"
  "for"
  "in"
  "bounded"
  "return"
  "match"
  "asm"
  "event"
  "reveal"
  "seal"
] @keyword

; RAM declarations
(sec_ram_declaration
  "ram" @keyword)

(ram_entry
  address: (integer_literal) @number)

; Builtin types
(primitive_type) @type.builtin

; Boolean literals
(boolean_literal) @constant.builtin

; Integer literals
(integer_literal) @number

; Comments
(line_comment) @comment

; Function definitions
(function_definition
  name: (identifier) @function)

; Function calls
(call_expression
  function: (module_path
    (identifier) @function .))

; Struct and event definitions
(struct_definition
  name: (identifier) @type)

(event_definition
  name: (identifier) @type)

; Struct init and emit/seal event names
(struct_init_expression
  name: (module_path (identifier) @type .))

(reveal_statement
  event: (identifier) @type)

(seal_statement
  event: (identifier) @type)

; Named types
(named_type
  (module_path (identifier) @type .))

; Parameters
(parameter
  name: (identifier) @variable.parameter)

; Let bindings
(let_statement
  pattern: (identifier) @variable)

; Const definitions
(const_definition
  name: (identifier) @constant)

; Attributes
(attribute) @attribute

; Program/module name
(program_declaration
  name: (identifier) @title)

(module_declaration
  name: (module_path (identifier) @title .))

; Use paths
(use_declaration
  (module_path (identifier) @module))

; Field access (dotted paths used as expressions)
; Field init names
(field_init
  name: (identifier) @property)

; Struct/event field declarations
(struct_field
  name: (identifier) @property)

(event_field
  name: (identifier) @property)

; Match arm patterns
(match_pattern) @constant

; Asm block target tag
(asm_annotation
  target: (identifier) @label)

; Asm effect annotation
(asm_effect) @number

; Asm body instructions
(asm_instruction
  (identifier) @keyword.directive)

; Operators
[
  "+"
  "*"
  "*."
  "=="
  "<"
  "&"
  "^"
  "/%"
  "="
  "->"
  ".."
  "=>"
] @operator

; Punctuation
["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," ":" ";"] @punctuation.delimiter
"." @punctuation.delimiter
"#" @punctuation.special
"_" @variable.builtin
