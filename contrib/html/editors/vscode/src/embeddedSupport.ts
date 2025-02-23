/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

interface EmbeddedRegion {
  languageId: string | undefined;
  start: number;
  end: number;
  attributeValue?: boolean;
}

enum TokenKind {
  Unknown,
  Colon,
  String,
  Identifier,
}

class BackScanner {
  currentToken: TokenKind = TokenKind.Unknown;
  tokenContent: string = "";

  constructor(
    private documentText: string,
    private offset: number,
  ) {
    this.scanBack();
  }

  getCurrentToken() {
    return this.currentToken;
  }

  scanBack() {
    let i = this.offset;
    this.currentToken = TokenKind.Unknown;
    while (i >= 0) {
      const ch = this.documentText[i];
      i--;
      //   console.log("scanBack", ch, this.currentToken, this.tokenContent);
      if (this.currentToken === TokenKind.Unknown) {
        if (ch === ":") {
          this.currentToken = TokenKind.Colon;
          this.tokenContent = ch;
          break;
        } else if (ch === '"') {
          this.currentToken = TokenKind.String;
          this.tokenContent = ch;
        } else if (/[a-zA-Z0-9\-]/.test(ch)) {
          this.currentToken = TokenKind.Identifier;
          this.tokenContent = ch;
        } else if (/\s/.test(ch)) {
          // ignore
        } else {
          break;
        }
      } else if (this.currentToken === TokenKind.String) {
        this.tokenContent = ch + this.tokenContent;
        if (ch === '"') {
          break;
        }
      } else if (this.currentToken === TokenKind.Identifier) {
        if (/[a-zA-Z0-9\-]/.test(ch)) {
          this.tokenContent = ch + this.tokenContent;
        } else {
          break;
        }
      }
    }
    this.offset = i;
  }
}

export function isInsideClassAttribute(documentText: string, offset: number) {
  console.log("isInsideClassAttribute", offset);

  // string start
  let start = offset - 1;
  while (start >= 0) {
    if (documentText[start] === '"') {
      let shashCount = 0;
      while (start > 0) {
        if (documentText[start - 1] === "\\") {
          shashCount++;
          start--;
        } else {
          break;
        }
      }

      if (shashCount % 2 === 0) {
        break;
      }

      start--;
    } else {
      start--;
    }
  }

  if (start >= 0 && documentText[start] === '"') {
    start -= 1;

    // find class attribute
    const reverseScanner = new BackScanner(documentText, start);
    if (reverseScanner.getCurrentToken() !== TokenKind.Colon) {
      return false;
    }
    reverseScanner.scanBack();
    if (reverseScanner.getCurrentToken() === TokenKind.Identifier) {
      console.log("found class attribute", reverseScanner.tokenContent);
      return reverseScanner.tokenContent === "class";
    }
    if (reverseScanner.getCurrentToken() === TokenKind.String) {
      console.log("found class attribute", reverseScanner.tokenContent);
      return reverseScanner.tokenContent === '"class"';
    }
  }

  return false;
}

export function parseRawBlockRegion(
  documentText: string,
  offset: number,
): EmbeddedRegion | undefined {
  let start = offset - 1;
  while (start >= 0) {
    if (documentText[start] === "`") {
      start -= 2;
      if (start < 0 || documentText.slice(start, start + 3) !== "```") {
        break;
      }

      let languageOffset = start + 3;
      let backtickStart = start;
      while (backtickStart > 0 && documentText[backtickStart - 1] === "`") {
        backtickStart -= 1;
      }

      let numOfBackticks = languageOffset - backtickStart;

      let languageStart = languageOffset;
      while (languageOffset < offset) {
        if (/\s/.test(documentText[languageOffset])) {
          break;
        }
        languageOffset++;
      }
      let languageId = documentText.slice(languageStart, languageOffset);

      console.log("numOfBackticks", numOfBackticks, languageOffset, languageId);

      let rawOffset = languageOffset;
      let rawEnd = languageOffset;
      let accumulatedBacktick = 0;

      while (rawEnd < documentText.length) {
        const isBacktick = documentText[rawEnd] === "`";
        rawEnd++;
        if (isBacktick) {
          accumulatedBacktick++;
        } else {
          if (accumulatedBacktick >= numOfBackticks) {
            break;
          }

          accumulatedBacktick = 0;
        }
      }

      if (accumulatedBacktick > rawEnd) {
        return;
      }
      rawEnd -= accumulatedBacktick;

      const rawContent = documentText.slice(rawOffset, rawEnd);

      console.log("raw content", languageId, rawOffset, rawEnd, rawContent);

      // return [languageId, rawOffset, rawEnd];
      return {
        languageId,
        start: rawOffset,
        end: rawEnd,
      };
    }
    start--;
  }

  return;
}

/**
 * Extract embedded regions from a document
 *
 * @param documentText The content of the document
 * @param regions The regions to embed
 * @param langId The language id to extract
 * @returns The content of the document with the regions embedded
 */
export function getVirtualContent(
  documentText: string,
  regions: EmbeddedRegion[],
  langId: string,
): string {
  // Keeps space.
  let content = documentText
    .split("\n")
    .map((line) => {
      return " ".repeat(line.length);
    })
    .join("\n");

  regions.forEach((r) => {
    if (r.languageId === langId) {
      content =
        content.slice(0, r.start) + documentText.slice(r.start, r.end) + content.slice(r.end);
    }
  });

  return content;
}
