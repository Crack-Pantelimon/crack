export default function myInitializer () {
    var old_loading_percent = 0;
    const loading_state = (show, text) => {
        if (show) {
            document.getElementById("loading-screen").style.display = "flex";
            document.getElementById("loading-screen").style.visibility = "visible";
            document.getElementById("loading-screen-text").textContent = text;
            console.log("Loading Wasm: ", text);
        } else {
            document.getElementById("loading-screen").style.display = "none";
            document.getElementById("loading-screen").style.visibility = "hidden";
            document.getElementById("loading-screen-text").textContent = "";
        }
    }
    return {
      onStart: () => {
        loading_state(true, "Started...");
        console.time("trunk-initializer");
      },
      onProgress: ({current, total}) => {
        if (!total) {
          loading_state(true, "Downloading Wasm... " + current + " bytes");
        } else {
          var loading_percent = Math.round((current/total) * 100);
          if (loading_percent != old_loading_percent) {
            loading_state(true, "Downloading Wasm... " + loading_percent + "% (" + Math.round(current/1024) + "K / " + Math.round(total/1024) + "K)");
            old_loading_percent = loading_percent;
          }
        }
      },
      onComplete: () => {
        loading_state(true, "Download Wasm Complete.");
        console.timeEnd("trunk-initializer");
      },
      onSuccess: (wasm) => {
        // loading_state(true, "Download Wasm Successful.");
        loading_state(false, "");
        console.log("Loading... successful!");
        // console.log("WebAssembly: ", wasm);
      },
      onFailure: (error) => {
        // loading_state(true, "Error: " + error);
        // if error contains the string "control flow" then it's not an error and we shouldn't print
        if (!(error+"").includes("control flow")) {
          console.warn("Loading... failed!", error);
        } else {
          console.log("control flow");
        }
      }
    }
  };