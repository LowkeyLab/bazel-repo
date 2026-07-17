interface DebugFunction {
  (...arguments_: unknown[]): void;
  enabled: boolean;
  namespace: string;
  destroy(): void;
  extend(suffix: string): DebugFunction;
  log(...arguments_: unknown[]): void;
}

interface DebugFactory {
  (namespace: string): DebugFunction;
  coerce(value: unknown): unknown;
  debug: DebugFactory;
  default: DebugFactory;
  disable(): string;
  enable(namespaces: string): void;
  enabled(namespace: string): boolean;
  log(...arguments_: unknown[]): void;
}

const createDebug = ((namespace: string): DebugFunction => {
  const debug = (() => undefined) as DebugFunction;
  debug.enabled = false;
  debug.namespace = namespace;
  debug.destroy = () => undefined;
  debug.extend = (suffix) => createDebug(`${namespace}:${suffix}`);
  debug.log = () => undefined;
  return debug;
}) as DebugFactory;

createDebug.coerce = (value) => value;
createDebug.debug = createDebug;
createDebug.default = createDebug;
createDebug.disable = () => "";
createDebug.enable = () => undefined;
createDebug.enabled = () => false;
createDebug.log = () => undefined;

export default createDebug;
