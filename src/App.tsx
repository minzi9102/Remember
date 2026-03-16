import { useEffect, useState } from "react";

import { bootstrapShell } from "./application/bootstrap";
import type { ShellState } from "./application/types";
import { RememberShell, RememberShellLoading } from "./ui/RememberShell";
import "./App.css";

function App() {
  const [shell, setShell] = useState<ShellState | null>(null);

  useEffect(() => {
    let isMounted = true;

    bootstrapShell()
      .then((loadedShell) => {
        if (isMounted) {
          setShell(loadedShell);
        }
      })
      .catch((error) => {
        console.error("[remember][ui] failed to bootstrap shell", error);
      });

    return () => {
      isMounted = false;
    };
  }, []);

  if (shell === null) {
    return <RememberShellLoading />;
  }

  return <RememberShell shell={shell} />;
}

export default App;
