#!/usr/bin/env python3
"""Generate the synthetic fixture projects used by the accuracy corpus.

Deterministic: running it again produces the same .tex files. Build the PDFs and
SyncTeX files afterwards with `cargo xtask fixtures`.
"""
import os
import random

ROOT = os.path.dirname(os.path.abspath(__file__))

SUBJECTS = ["the estimator", "our procedure", "the proposed method", "this baseline", "the sampling scheme",
            "the calibration step", "the filtering stage", "the reference model", "each sensor", "the field campaign",
            "the offline variant", "the robust fit", "the smoothing kernel", "the bootstrap", "the validation set"]
VERBS = ["reduces", "increases", "stabilises", "affects", "dominates", "explains", "approximates", "outperforms",
         "complements", "constrains", "underestimates", "overestimates", "reflects", "requires", "supports"]
OBJECTS = ["the variance of the residuals", "the efficiency of the pipeline", "the bias near the boundary",
           "the coverage of the intervals", "the difficult cases in winter", "the influence of outliers",
           "the runtime on large inputs", "the accuracy of the forecasts", "the stability of the estimates",
           "the effect of missing values", "the drift between deployments", "the fit at low signal levels",
           "the trade-off between speed and precision", "the sensitivity to the window length",
           "the agreement with manual labels"]
TAILS = ["in most configurations", "when the noise is heavy-tailed", "for the offshore stations",
         "after the first iteration", "under realistic assumptions", "without additional tuning",
         "across all three datasets", "at the cost of extra computation", "in the second half of the record",
         "compared with earlier work", "for short observation windows", "once the drift is removed"]
CONNECT = ["However,", "In addition,", "As a result,", "In contrast,", "Moreover,", "For this reason,",
           "Interestingly,", "In practice,", "Taken together,", "By construction,"]
NAMES = ["Hale", "Okafor", "Lindqvist", "Moreau", "Tanaka", "Silva", "Brennan", "Kowalczyk", "Haddad", "Novak"]


UNIQUE = {"on": False, "n": 0}


def sentence(rng, i):
    s = f"{rng.choice(SUBJECTS).capitalize()} {rng.choice(VERBS)} {rng.choice(OBJECTS)} {rng.choice(TAILS)}"
    if UNIQUE["on"]:
        # Performance fixture: make every sentence findable by a unique case number.
        UNIQUE["n"] += 1
        s += f" in case {UNIQUE['n']}"
    r = rng.random()
    if r < 0.12:
        s += f", as shown in Table~\\ref{{tab:t{rng.randint(1, 3)}}}"
    elif r < 0.22:
        s += f" \\citep{{ref{rng.randint(1, 40)}}}"
    elif r < 0.30:
        s += f" with $\\alpha_{{{rng.randint(1, 9)}}} = {rng.randint(1, 99) / 100}$"
    elif r < 0.36:
        s += f", following \\emph{{{rng.choice(NAMES)}}}"
    elif r < 0.40:
        s += f"\\footnote{{See the appendix of run {i} for details.}}"
    s += "."
    if rng.random() < 0.3:
        s = f"{rng.choice(CONNECT)} {s[0].lower()}{s[1:]}"
    return s


def paragraph(rng, i, n=None):
    n = n or rng.randint(3, 7)
    lines = []
    for k in range(n):
        s = sentence(rng, i * 10 + k)
        # Wrap source lines the way people write them: roughly one clause per line.
        words = s.split(" ")
        cur = []
        for w in words:
            cur.append(w)
            if len(" ".join(cur)) > rng.randint(45, 78):
                lines.append(" ".join(cur))
                cur = []
        if cur:
            lines.append(" ".join(cur))
    return "\n".join(lines)


def chapter(rng, n, title, sections):
    out = [f"\\chapter{{{title}}}\\label{{ch:{n}}}", ""]
    for si, st in enumerate(sections):
        out += [f"\\section{{{st}}}\\label{{sec:{n}.{si}}}", ""]
        for p in range(rng.randint(3, 5)):
            out += [paragraph(rng, n * 100 + si * 10 + p), ""]
            if rng.random() < 0.15:
                out += ["\\begin{itemize}", f"  \\item {sentence(rng, 1)}", f"  \\item {sentence(rng, 2)}",
                        "\\end{itemize}", ""]
            if rng.random() < 0.12:
                out += ["\\begin{equation}", f"  y_i = \\beta_0 + \\beta_1 x_i + \\varepsilon_i \\label{{eq:{n}{si}{p}}}",
                        "\\end{equation}", ""]
        if si == 0:
            out += ["\\begin{table}[ht]", "\\centering", "\\begin{tabular}{lrr}", "Setting & Error & Time \\\\",
                    f"Baseline & 0.{rng.randint(10, 99)} & {rng.randint(2, 9)}.1 \\\\",
                    f"Proposed & 0.{rng.randint(10, 99)} & {rng.randint(2, 9)}.4 \\\\", "\\end{tabular}",
                    f"\\caption{{Results for setting {n}.}}\\label{{tab:t{n}}}", "\\end{table}", ""]
        if si == 1:
            out += ["\\begin{figure}[ht]", "\\centering", "\\rule{0.6\\linewidth}{4cm}",
                    f"\\caption{{Overview of stage {n}.}}\\label{{fig:f{n}}}", "\\end{figure}", ""]
    return "\n".join(out) + "\n"


def thesis_long():
    rng = random.Random(20261125)
    d = os.path.join(ROOT, "thesis-long")
    os.makedirs(os.path.join(d, "chapters"), exist_ok=True)
    titles = [("Introduction", ["Motivation", "Contributions", "Outline"]),
              ("Background", ["Sensor networks", "Robust statistics", "Related work"]),
              ("Method", ["Estimating the threshold", "Calibration", "Complexity"]),
              ("Experiments", ["Datasets", "Results", "Ablations"]),
              ("Discussion", ["Limitations", "Future work"])]
    inputs = []
    for n, (t, secs) in enumerate(titles, start=1):
        name = f"ch{n}-{t.lower()}"
        with open(os.path.join(d, "chapters", name + ".tex"), "w") as f:
            f.write(chapter(rng, n, t, secs))
        inputs.append(f"\\input{{chapters/{name}}}")
    with open(os.path.join(d, "main.tex"), "w") as f:
        f.write("\\documentclass[11pt,a4paper]{report}\n\\usepackage{amsmath}\n\\usepackage[numbers]{natbib}\n"
                "\\newcommand{\\method}{the proposed method}\n\\title{Robust Thresholds for Sensor Data}\n"
                "\\author{A. Student}\n\\begin{document}\n\\maketitle\n\\tableofcontents\n"
                + "\n".join(inputs) + "\n\\end{document}\n")


def article_twocol():
    rng = random.Random(77)
    d = os.path.join(ROOT, "article-twocolumn")
    os.makedirs(d, exist_ok=True)
    body = []
    for si, st in enumerate(["Introduction", "Method", "Evaluation", "Conclusion"]):
        body += [f"\\section{{{st}}}", ""]
        for p in range(rng.randint(4, 6)):
            body += [paragraph(rng, si * 10 + p), ""]
    with open(os.path.join(d, "main.tex"), "w") as f:
        f.write("\\documentclass[10pt,twocolumn]{article}\n\\usepackage{amsmath}\n\\usepackage[numbers]{natbib}\n"
                "\\begin{document}\n\\title{Short Paper on Drift}\\author{B. Author}\\date{}\\maketitle\n"
                + "\n".join(body) + "\n\\end{document}\n")


def typst_sentence(rng, i):
    s = f"{rng.choice(SUBJECTS).capitalize()} {rng.choice(VERBS)} {rng.choice(OBJECTS)} {rng.choice(TAILS)}"
    r = rng.random()
    if r < 0.12:
        s += f", as shown in @tab-{rng.randint(1, 3)}"
    elif r < 0.22:
        s += f" with $alpha_{rng.randint(1, 9)} = {rng.randint(1, 99) / 100}$"
    elif r < 0.30:
        s += f", following _{rng.choice(NAMES)}_"
    elif r < 0.36:
        s += f"#footnote[See run {i} for details.]"
    s += "."
    if rng.random() < 0.3:
        s = f"*{rng.choice(CONNECT)}* {s[0].lower()}{s[1:]}"
    return s


def typst_paper():
    rng = random.Random(4242)
    d = os.path.join(ROOT, "typst-paper")
    os.makedirs(os.path.join(d, "sections"), exist_ok=True)
    includes = []
    for n, title in enumerate(["Introduction", "Method", "Results", "Discussion"], start=1):
        body = [f"= {title}", ""]
        for k in range(3):
            body += [f"== Part {n}.{k + 1}", ""]
            for p in range(rng.randint(2, 4)):
                lines = []
                for q in range(rng.randint(3, 6)):
                    lines.append(typst_sentence(rng, n * 100 + k * 10 + q))
                body += ["\n".join(lines), ""]
        if n == 3:
            body += ["#figure(table(columns: 2, [Setting], [Error], [Baseline], [0.31], [Proposed], [0.12]), caption: [Results.]) <tab-1>", ""]
        name = f"sections/s{n}.typ"
        with open(os.path.join(d, name), "w") as f:
            f.write("\n".join(body) + "\n")
        includes.append(f'#include "{name}"')
    with open(os.path.join(d, "main.typ"), "w") as f:
        f.write('#set page(paper: "a4", numbering: "1")\n#set text(font: "Libertinus Serif", size: 11pt)\n#set par(justify: true)\n#set heading(numbering: "1.1")\n\n#align(center, text(16pt)[*Robust Thresholds in Typst*])\n\n'
                + "\n".join(includes) + "\n\n#figure(table(columns: 2, [A], [1], [B], [2]), caption: [Second table.]) <tab-2>\n#figure(table(columns: 2, [C], [3]), caption: [Third table.]) <tab-3>\n")


def thesis_perf(out_dir, chapters=80):
    """A ~300-page thesis for performance tests. Not committed; built on demand."""
    rng = random.Random(300)
    UNIQUE["on"] = True
    d = out_dir
    os.makedirs(os.path.join(d, "chapters"), exist_ok=True)
    inputs = []
    for n in range(1, chapters + 1):
        name = f"ch{n:03d}"
        secs = [f"Part {k}" for k in range(1, 4)]
        with open(os.path.join(d, "chapters", name + ".tex"), "w") as f:
            f.write(chapter(rng, n, f"Chapter {n}", secs))
        inputs.append(f"\\input{{chapters/{name}}}")
    with open(os.path.join(d, "main.tex"), "w") as f:
        f.write("\\documentclass[11pt,a4paper]{report}\n\\usepackage{amsmath}\n\\usepackage[numbers]{natbib}\n"
                "\\begin{document}\n" + "\n".join(inputs) + "\n\\end{document}\n")


if __name__ == "__main__":
    import sys
    if len(sys.argv) > 2 and sys.argv[1] == "--perf":
        thesis_perf(sys.argv[2])
        print("generated performance fixture in", sys.argv[2])
    else:
        thesis_long()
        article_twocol()
        typst_paper()
        print("generated thesis-long, article-twocolumn and typst-paper")
