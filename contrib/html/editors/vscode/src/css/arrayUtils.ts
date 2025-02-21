let XXH = require("xxhashjs").h32;

export function flatten<T>(nestedArray: T[][]): T[] {
  if (nestedArray.length === 0) {
    throw new RangeError("Can't flatten an empty array.");
  } else {
    return nestedArray.reduce((a, b) => a.concat(b));
  }
}

export function distinct<T>(items: T[] | Thenable<T[]>): Thenable<T[]> {
  return Promise.resolve(items).then((items) => Array.from(new Set(items)));
}

export function distinctByXXHash<T>(items: T[] | Thenable<T[]>): Thenable<T[]> {
  const initialValue = {
    distinctItems: <T[]>[],
    hashSet: new Set(),
  };

  const accumulatorPromise = Promise.resolve(items).then((items) =>
    items.reduce((acc, item) => {
      const hash = XXH(item, 0x1337).toNumber();

      if (!acc.hashSet.has(hash)) {
        acc.distinctItems.push(item);
        acc.hashSet.add(hash);
      }

      return acc;
    }, initialValue),
  );

  return accumulatorPromise.then((accumulator) => accumulator.distinctItems);
}
