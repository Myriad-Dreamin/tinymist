import type { FontResources } from "./fonts";

export const MOCK_DATA: FontResources = {
  sources: [
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\SongTi-Regular.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\SongTi-Bold.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\TimesNewRoman-Regular.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\TimesNewRoman-Italic.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\TimesNewRoman-Bold.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\MicrosoftYaHei-Regular.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\MicrosoftYaHei-Light.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\Arial-Condensed.ttf",
    },
  ],
  families: [
    {
      name: "Song Ti",
      infos: [
        {
          name: "Song Ti Regular",
          weight: 400,
          style: "normal",
          stretch: 1000,
          source: 0,
        },
        {
          name: "Song Ti Bold",
          weight: 700,
          style: "normal",
          stretch: 1000,
          source: 1,
        },
      ],
    },
    {
      name: "Times New Roman",
      infos: [
        {
          name: "Times New Roman",
          weight: 400,
          style: "normal",
          stretch: 1000,
          source: 2,
        },
        {
          name: "Times New Roman Italic",
          weight: 400,
          style: "italic",
          stretch: 1000,
          source: 3,
        },
        {
          name: "Times New Roman Bold",
          weight: 700,
          style: "normal",
          stretch: 1000,
          source: 4,
        },
      ],
    },
    {
      name: "Microsoft YaHei",
      infos: [
        {
          name: "Microsoft YaHei",
          weight: 400,
          style: "normal",
          stretch: 1000,
          source: 5,
        },
        {
          name: "Microsoft YaHei Light",
          weight: 300,
          style: "normal",
          stretch: 1000,
          source: 6,
        },
      ],
    },
    {
      name: "Arial",
      infos: [
        {
          name: "Arial Condensed",
          weight: 400,
          style: "normal",
          stretch: 750,
          source: 7,
        },
      ],
    },
  ],
};
