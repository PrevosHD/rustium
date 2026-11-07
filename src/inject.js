// Runs inside every service page: routes web notifications to rustium's native notifications.
(() => {
  const send = (title, options) =>
    window.__TAURI_INTERNALS__
      ?.invoke('notify', { title: String(title ?? ''), body: String(options?.body ?? '') })
      .catch((e) => console.warn('rustium: notification not forwarded', e));

  class RustiumNotification extends EventTarget {
    static permission = 'granted';

    static requestPermission(callback) {
      callback?.('granted');
      return Promise.resolve('granted');
    }

    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = options.body ?? '';
      this.onclick = null;
      send(title, options);
    }

    close() {}
  }

  window.Notification = RustiumNotification;
  if (window.ServiceWorkerRegistration) {
    ServiceWorkerRegistration.prototype.showNotification = (title, options) => {
      send(title, options);
      return Promise.resolve();
    };
  }
})();
