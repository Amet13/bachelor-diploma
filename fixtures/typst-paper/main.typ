#set page(paper: "a4", numbering: "1")
#set text(font: "Libertinus Serif", size: 11pt)
#set par(justify: true)
#set heading(numbering: "1.1")

#align(center, text(16pt)[*Robust Thresholds in Typst*])

#include "sections/s1.typ"
#include "sections/s2.typ"
#include "sections/s3.typ"
#include "sections/s4.typ"

#figure(table(columns: 2, [A], [1], [B], [2]), caption: [Second table.]) <tab-2>
#figure(table(columns: 2, [C], [3]), caption: [Third table.]) <tab-3>
