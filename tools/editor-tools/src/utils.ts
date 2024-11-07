const bytes2utf8 = new TextDecoder("utf-8");
const utf82bytes = new TextEncoder();

/**
 * Base64 to UTF-8
 * @param encoded Base64 encoded string
 * @returns UTF-8 string
 */
export const base64Decode = (encoded: string) =>
  bytes2utf8.decode(Uint8Array.from(atob(encoded), (m) => m.charCodeAt(0)));

/**
 * UTF-8 to Base64
 * @param utf8Str UTF-8 string
 * @returns Base64 encoded string
 */
export const base64Encode = (utf8Str: string) =>
  btoa(
    Array.from(utf82bytes.encode(utf8Str), (c) => String.fromCharCode(c)).join(
      ""
    )
  );
