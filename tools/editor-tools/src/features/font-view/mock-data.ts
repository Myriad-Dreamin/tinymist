import type { FontResources } from "./fonts";

export const MOCK_DATA: FontResources = {
  sources: [
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
      kind: "memory",
      name: "MicrosoftYaHei-Regular",
    },
    {
      kind: "memory",
      name: "MicrosoftYaHei-Light",
    },
  ],
  families: [
    {
      name: "Times New Roman",
      infos: [
        {
          name: "Times New Roman",
          weight: 400,
          style: "normal",
          stretch: 1000,
          source: 0,
        },
        {
          name: "Times New Roman Italic",
          weight: 400,
          style: "italic",
          stretch: 1000,
          source: 1,
        },
        {
          name: "Times New Roman Bold",
          weight: 700,
          style: "normal",
          stretch: 1000,
          source: 2,
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
          source: 3,
        },
        {
          name: "Microsoft YaHei Light",
          weight: 300,
          style: "normal",
          stretch: 1000,
          source: 4,
        },
      ],
    },
  ],
};
