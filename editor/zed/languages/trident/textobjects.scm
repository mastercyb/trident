; Text objects for Helix (vif/vaf, vic/vac, etc.)

; Functions
(function_definition) @function.around
(function_definition
  body: (block) @function.inside)

; Structs and events as "class" objects
(struct_definition) @class.around
(event_definition) @class.around

; Loops
(for_statement) @loop.around
(for_statement
  body: (block) @loop.inside)

; Conditionals
(if_statement) @conditional.around
(match_statement) @conditional.around
(match_arm) @entry.around

; Parameters
(parameter) @parameter.inside

; Blocks
(block) @block.inside

; Comments
(line_comment) @comment.around
