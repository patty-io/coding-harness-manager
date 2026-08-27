import { Route, Routes } from "react-router-dom";
import Sidebar from "./components/Sidebar";
import InventoryScreen from "./screens/InventoryScreen";
import ImportWizard from "./screens/ImportWizard";
import ProvidersScreen from "./screens/ProvidersScreen";
import ProviderDetailScreen from "./screens/ProviderDetailScreen";
import ModelsScreen from "./screens/ModelsScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";

const PLACEHOLDERS = [
  "mcp",
  "skills",
  "profiles",
  "sets",
  "changes",
  "history",
  "doctor",
  "settings",
];

export default function App() {
  return (
    <div className="flex h-screen">
      <Sidebar />
      <main className="flex-1 overflow-auto bg-gray-50 p-6">
        <Routes>
          <Route path="/" element={<DashboardScreen />} />
          <Route path="/scan" element={<InventoryScreen />} />
          <Route path="/import" element={<ImportWizard />} />
          <Route path="/providers" element={<ProvidersScreen />} />
          <Route path="/providers/:id" element={<ProviderDetailScreen />} />
          <Route path="/models" element={<ModelsScreen />} />
          {PLACEHOLDERS.map((p) => (
            <Route
              key={p}
              path={`/${p}`}
              element={<PlaceholderScreen title={p} />}
            />
          ))}
        </Routes>
      </main>
    </div>
  );
}