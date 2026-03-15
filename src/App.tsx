import { bootstrapShell } from "./application/bootstrap";
import { RememberShell } from "./ui/RememberShell";
import "./App.css";

function App() {
  const shell = bootstrapShell();
  return <RememberShell shell={shell} />;
}

export default App;
