/// path: references.yaml
harry:
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
/// contains:harry,Harry Potter and the Order of the Phoenix,electronic,Ishkur's Guide to Electronic Music
/// compile: true

#cite(<harry>) /* range -2..-1 */

#bibliography("references.yaml")
