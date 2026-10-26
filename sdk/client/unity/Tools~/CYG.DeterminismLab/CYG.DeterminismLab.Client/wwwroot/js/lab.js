window.cygLab = {
    origin: function () {
        return window.location.origin;
    },
    getItem: function (key) {
        try {
            return localStorage.getItem(key);
        } catch {
            return null;
        }
    },
    setItem: function (key, value) {
        try {
            localStorage.setItem(key, value);
        } catch {
            // Private mode or blocked storage: the lab still runs, it just
            // gets a fresh device id every visit.
        }
    },
    userAgent: function () {
        return navigator.userAgent;
    },
    platform: function () {
        return navigator.platform || "unknown";
    },
    language: function () {
        return navigator.language || "unknown";
    }
};
