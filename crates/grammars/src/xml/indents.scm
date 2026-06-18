(STag
  ">" @end) @indent

(EmptyElemTag
  "/>" @end) @indent

(element
  (STag) @start
  (ETag)? @end) @indent
