import type { ExtensionContext } from "../../../state";
import { substituteTemplateString } from "../../../util";
import { defineEditorTool } from "..";

export const USER_PACKAGE_VERSION = "0.0.1";

interface Versioned<T> {
  version: string;
  data: T;
}

interface PackageData {
  [ns: string]: {
    [packageName: string]: {
      isFavorite: boolean;
    };
  };
}

function getUserPackageData(context: ExtensionContext) {
  const defaultPackageData: Versioned<PackageData> = {
    version: USER_PACKAGE_VERSION,
    data: {},
  };

  const userPackageData = context.globalState.get("userPackageData", defaultPackageData);
  if (userPackageData?.version !== USER_PACKAGE_VERSION) {
    return defaultPackageData;
  }

  return userPackageData;
}

export default defineEditorTool({
  id: "template-gallery",
  command: {
    command: "tinymist.showTemplateGallery",
    title: "Template Gallery",
    tooltip: "Show Template Gallery",
  },

  transformHtml: (html, { context }) => {
    const userPackageData = getUserPackageData(context);
    const packageData = JSON.stringify(userPackageData.data);
    return substituteTemplateString(html, { ":[[preview:FavoritePlaceholder]]:": packageData });
  },
});
