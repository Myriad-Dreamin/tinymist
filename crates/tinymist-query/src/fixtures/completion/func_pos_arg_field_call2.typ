/// contains: text, mode

#show regex(":\S+:"): it => eval("emoji." + /* range after 5..6 */ it.te.slice(1, it.text.len() - 1))

:cat: :anger:
