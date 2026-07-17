import EventEmitter from "node:events";

// CommonJS packages expect require("events") to return EventEmitter itself.
// Rollup otherwise supplies the ESM namespace object when Nitro bundles them.
export default EventEmitter;
