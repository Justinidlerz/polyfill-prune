// A prebundled library with an inline native-API fallback.
export const assign =
  Object.assign ||
  function legacyAssign(target, ...sources) {
    for (const source of sources) {
      for (const key of Object.keys(source)) target[key] = source[key];
    }
    return target;
  };
