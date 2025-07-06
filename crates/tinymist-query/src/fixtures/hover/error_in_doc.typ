/// *
#let my-fun(mode: "typ", setting: it => it, note) = {
  touying-fn-wrapper(utils.my-fun, mode: mode, setting: setting, note)
}

#(/* ident after */ my-fun);
