import { Route, Routes, Navigate } from "react-router-dom";
import { useBackForwardNavigation } from "./hooks/useBackForwardNavigation";
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
import SettingsScreen from "./screens/SettingsScreen";
import HarnessProviderDetailScreen from "./screens/HarnessProviderDetailScreen";
import { ToastViewport } from "./components/Toast";
import NotFoundScreen from "./screens/NotFoundScreen";

export default function App() {
  useBackForwardNavigation();
  return (
    <div className="flex h-screen bg-slate-950 text-slate-200">
      <Sidebar />
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded focus:bg-blue-600 focus:px-3 focus:py-2"
      >
        Skip to content
      </a>
      <main id="main-content" className="flex-1 overflow-auto bg-slate-900 p-6 text-slate-200">
        <Routes>
          <Route path="/" element={<DashboardScreen />} />
          <Route path="/harnesses" element={<HarnessesScreen />} />
          <Route path="/harnesses/:id" element={<HarnessDetailScreen />} />
          <Route path="/harnesses/:id/providers/:providerName" element={<HarnessProviderDetailScreen />} />
          <Route path="/scan" element={<Navigate to="/harnesses" replace />} />
          <Route path="/import" element={<ImportWizard />} />
          <Route path="/providers" element={<ProvidersScreen />} />
          <Route path="/providers/:id" element={<ProviderDetailScreen />} />
          <Route path="/models" element={<ModelsScreen />} />
          <Route path="/mcp" element={<McpScreen />} />
          <Route path="/skills" element={<SkillsScreen />} />
          <Route path="/profiles" element={<ProfilesScreen />} />
          <Route path="/history" element={<HistoryScreen />} />
          <Route path="/settings" element={<SettingsScreen />} />
          <Route path="/pending" element={<Navigate to="/history" replace />} />
          <Route path="*" element={<NotFoundScreen />} />
        </Routes>
      </main>
      <ToastViewport />
    </div>
  );
}
