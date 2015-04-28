all: build run

build:
	latexmk -xelatex \
	-synctex=1 main.tex
	
run:
	# Я использую atril для просмотра PDF
	atril main.pdf &
	
clean:
	rm *.aux \
	*.fdb_latexmk \
	*.fls \
	*.log \
	*.out \
	*.synctex.gz \
	*.toc \
	*.tex~
