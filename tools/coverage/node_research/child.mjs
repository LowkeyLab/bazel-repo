import { childOnly } from "./child_only.js";
if (childOnly() !== 41) throw new Error("Child assertion failed");
