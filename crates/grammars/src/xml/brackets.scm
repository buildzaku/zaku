(("<" @open
  "/>" @close)
  (#set! rainbow.exclude))

(("<" @open
  ">" @close)
  (#set! rainbow.exclude))

(("</" @open
  ">" @close)
  (#set! rainbow.exclude))

(("\"" @open
  "\"" @close)
  (#set! rainbow.exclude))

((element
  (STag) @open
  (ETag) @close)
  (#set! newline.only)
  (#set! rainbow.exclude))
