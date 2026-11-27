import { AppProviders } from "./app/AppProviders";
import { AppWorkspace } from "./app/AppWorkspace";
import "./App.css";

function App() {
  return (
    <AppProviders>
      <AppWorkspace />
    </AppProviders>
  );
}

export default App;
