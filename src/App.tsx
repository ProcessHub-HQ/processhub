import "@processhub/design-system/tokens.css";
import "@processhub/design-system/base.css";
import "@processhub/design-system/components.css";
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
