; Indent rules for Helix auto-indent

[
  (function_definition)
  (struct_definition)
  (event_definition)
  (if_statement)
  (for_statement)
  (match_statement)
  (match_arm)
  (block)
  (asm_block)
] @indent

[
  "}"
  ")"
  "]"
] @outdent
