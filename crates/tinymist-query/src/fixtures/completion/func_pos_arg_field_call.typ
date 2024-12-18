/// contains: text, mode

#show regex(":\S+:"): it => eval("emoji." + /* range after 3..4 */it..slice(1, it.text.len() - 1))

:cat: :anger: