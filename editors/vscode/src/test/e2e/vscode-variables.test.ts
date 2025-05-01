import { Context } from ".";
import { CodeVariableContext, vscodeVariables } from "../../vscode-variables";

export async function getTests(ctx: Context) {
  // await ctx.openWorkspace("simple-docs");
  await ctx.suite("vscodeVariables", async (suite) => {
    suite.addTest("emptyString", async () => {
      ctx.expect(vscodeVariables("")).to.eq(``);
      ctx.expect(vscodeVariables("", true)).to.eq(``);
    });
    suite.addTest("variable", async () => {
      const context = CodeVariableContext.test({
        absoluteFilePath: "/a/b/c/d.txt",
      });
      ctx.expect(vscodeVariables("${file}", true, context)).to.eq(`/a/b/c/d.txt`);
      ctx.expect(vscodeVariables("${fileBasename}", true, context)).to.eq(`d.txt`);
    });
  });
}
