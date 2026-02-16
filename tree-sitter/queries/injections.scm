; Language injection: TASM assembly inside asm blocks

(asm_block
  body: (asm_body) @injection.content
  (#set! injection.language "asm"))
