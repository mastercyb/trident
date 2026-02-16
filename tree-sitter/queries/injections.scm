; Language injection: TASM assembly inside asm blocks

(asm_block
  (asm_body) @injection.content
  (#set! injection.language "asm"))
