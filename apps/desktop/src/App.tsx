import { Route, Routes, Navigate } from "react-router-dom";
import Sidebar from "./components/Sidebar";
import HarnessesScreen from "./screens/HarnessesScreen";
import HarnessDetailScreen from "./screens/HarnessDetailScreen";
import ImportWizard from "./screens/ImportWizard";
import ProvidersScreen from "./screens/ProvidersScreen";
import ProviderDetailScreen from "./screens/ProviderDetailScreen";
import ModelsScreen from "./screens/ModelsScreen";
import McpScreen from "./screens/McpScreen";
import SkillsScreen from "./screens/SkillsScreen";
import HistoryScreen from "./screens/HistoryScreen";
import ProfilesScreen from "./screens/ProfilesScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";

const PLACEHOLDERS = [
  "pending",
  "doctor",
  "settings",
];

export default function App() {
  return (
    <div className="flex h-screen bg-slate-950 text-slate-200">
      <Sidebar />
      <main className="flex-1 overflow-auto bg-slate-900 p-6 text-slate-200">
        <Routes>
          <Route path="/" element={<DashboardScreen />} />
          <Route path="/harnesses" element={<HarnessesScreen />} />
          <Route path="/harnesses/:id" element={<HarnessDetailScreen />} />
          <Route path="/scan" element={<Navigate to="/harnesses" replace />} />
          <Route path="/import" element={<ImportWizard />} />
          <Route path="/providers" element={<ProvidersScreen />} />
          <Route path="/providers/:id" element={<ProviderDetailScreen />} />
          <Route path="/models" element={<ModelsScreen />} />
          <Route path="/mcp" element={<McpScreen />} />
          <Route path="/skills" element={<SkillsScreen />} />
          <Route path="/profiles" element={<ProfilesScreen />} />
          <Route path="/history" element={<HistoryScreen />} />
          <Route path="/profiles" element={<ProfilesScreen />} />
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