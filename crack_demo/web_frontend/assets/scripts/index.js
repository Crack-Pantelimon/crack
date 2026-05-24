function sleep(ms) {
    console.log("sleeping ", ms, "ms")
    return new Promise(resolve => setTimeout(resolve, ms));
}

const registerServiceWorker = async () => {
  if ("serviceWorker" in navigator) {
    try {
      const registration = await navigator.serviceWorker.register("/assets/scripts/worker.js", {
        scope: "/assets/scripts/",
      });
      if (registration.installing) {
        console.log("Service worker installing");
        await sleep(250);
        window.location.reload();
      } else if (registration.waiting) {
        console.log("Service worker installed");
        await sleep(250);
        window.location.reload();

      } else if (registration.active) {
        console.log("Service worker active");
        use_registration(registration);
      }
    } catch (error) {
      console.error(`Registration failed with ${error}`);
      
        await sleep(3250);
        window.location.reload();

    }
  }
};

// …

registerServiceWorker();
