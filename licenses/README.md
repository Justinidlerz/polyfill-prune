# Third-party data

`native/data/compat.json` combines:

- [MDN Browser Compat Data](https://github.com/mdn/browser-compat-data), CC0 1.0. See [license](MDN-BCD-LICENSE).
- API declarations from [TypeScript](https://github.com/microsoft/TypeScript), Apache 2.0. See [license](TYPESCRIPT-LICENSE.txt).

- API locations and member kinds from [W3C Webref](https://github.com/w3c/webref), MIT. See [license](WEBREF-LICENSE). The normalized snapshot is stored in `native/data/web-apis.json`.

Exact source package versions and the Webref commit are stored in the generated data and returned by `version()`. The bundled compatibility data is redistributed under its source licenses; project source code uses MIT.
