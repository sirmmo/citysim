// Wavedash (wavedash.com) hook. On Wavedash the page has a global `Wavedash` object and the game
// stays behind Wavedash's loading screen until it calls Wavedash.init(); anywhere else (GitHub
// Pages, a local server) there is no such object and these do nothing. A plain script, so that
// module and classic pages alike can call:
//   window.WavedashHook?.progress(0.5)   // loading, from 0 to 1
//   window.WavedashHook?.ready()         // loaded: show the game (once; later calls do nothing)
(function () {
  var done = false;
  function call(name, arg) {
    var run = function (sdk) {
      try {
        var r = sdk && typeof sdk[name] === 'function' ? sdk[name](arg) : null;
        if (r && typeof r.then === 'function') r.then(null, function () {});
      } catch (e) { /* never let the platform break the game */ }
    };
    try {
      var sdk = window.Wavedash;
      if (!sdk) return;
      if (typeof sdk[name] === 'function') run(sdk);
      else if (typeof sdk.then === 'function') sdk.then(run, function () {});
    } catch (e) { /* no Wavedash */ }
  }
  window.WavedashHook = {
    progress: function (x) {
      if (!done) call('updateLoadProgressZeroToOne', Math.min(1, Math.max(0, Number(x) || 0)));
    },
    ready: function () {
      if (done) return;
      done = true;
      call('updateLoadProgressZeroToOne', 1);
      call('init');
    },
  };
  // An error while loading shows the page (and the game's own message, if it has one) rather
  // than leave the player in front of Wavedash's loading screen.
  window.addEventListener('error', window.WavedashHook.ready);
  window.addEventListener('unhandledrejection', window.WavedashHook.ready);
})();
