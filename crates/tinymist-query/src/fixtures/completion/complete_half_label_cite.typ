/// path: references.yaml
tarry:
    type: Book
    title: Harry Potter and the Order of the Phoenix
    author: Rowling, J. K.
    volume: 5
    page-total: 768
    date: 2003-06-21

electronic:
    type: Web
    title: Ishkur's Guide to Electronic Music
    serial-number: v2.5
    author: Ishkur
    url: http://www.techno.org/electronic-music-guide/

-----
/// path: base.typ

#set heading(numbering: "1.1")

= H <test>

#bibliography("references.yaml")

-----
/// contains: tarry, test
/// compile: base.typ

#cite(<t /* range -2..-1 */)
