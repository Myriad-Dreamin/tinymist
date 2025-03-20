// This is different from `wordSeparators`. We need to document difference here.
// todo: document difference here.
//
// https://code.visualstudio.com/api/language-extensions/language-configuration-guide#word-pattern
// export const wordPattern = /(-?\d*\.\d\w*)|([^`~!@#$%^&*()=+[{\]}\\|;:'",.<>/?\s]+)/;
export const wordPattern =
  /(-?\d*\.\d\w*)|(-?\d+\.(?:\d\w*)?)|([^`~!@#$%^&*()=+\-[{\]}\\|;:'",.<>/?\s][^`~!@#$%^&*()=+[{\]}\\|;:'",.<>/?\s]*)/;
