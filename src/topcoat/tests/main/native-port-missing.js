// Explicit coverage gap: no retired application controller is used as a subject.
class NativePortMissing extends Error {
  constructor(feature) {
    super(`Native ${feature} test adapter is unfinished; the intermediate JavaScript implementation was removed.`);
    this.name = 'NativePortMissing';
    this.code = 'ERR_NATIVE_PORT_MISSING';
  }
}
function unavailable(feature) { throw new NativePortMissing(feature); }
module.exports = {NativePortMissing, unavailable};
