/// Speaker notes are a way to add additional information to your slides that is not visible to the audience. This can be useful for providing additional context or reminders to yourself.
///
/// == Example
///
/// #example(```typ
/// #speaker-note[This is a speaker note]
/// ```)
#let speaker-note(mode: "typ", setting: it => it, note) = {
  touying-fn-wrapper(utils.speaker-note, mode: mode, setting: setting, note)
}

#(/* ident after */ speaker-note);
