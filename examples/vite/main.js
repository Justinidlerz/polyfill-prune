import { assign } from 'polyfill-prune-example-vendor';

if (!Array.prototype.at) {
  Array.prototype.at = function legacyAt(index) {
    return this[index < 0 ? this.length + index : index];
  };
}
document.querySelector('#output').textContent = JSON.stringify(
  assign({}, { result: [1, 2].at(-1) }),
);
