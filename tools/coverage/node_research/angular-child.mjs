import { childOnly } from "./child_only.ts";
if (childOnly() !== 41) throw Error("child");
