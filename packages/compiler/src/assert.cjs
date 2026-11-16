// Node's `assert`, which a Babel helper loads with require() and calls, for the browser build.
module.exports = function assert(ok, message) {
  if (!ok) throw new Error(message || "assertion failed");
};
