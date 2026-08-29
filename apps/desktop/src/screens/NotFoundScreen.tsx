import { Link, useLocation } from "react-router-dom";

export default function NotFoundScreen() {
  const location = useLocation();
  return (
    <div className="mx-auto max-w-xl py-12 text-center">
      <p className="text-sm uppercase tracking-wide text-slate-500">404</p>
      <h1 className="mt-2 text-2xl font-bold text-slate-100">Page not found</h1>
      <p className="mt-2 text-sm text-slate-400">
        There is no Coding Harness Manager view at <code>{location.pathname}</code>.
      </p>
      <Link
        to="/"
        className="mt-5 inline-block rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-500"
      >
        Back to dashboard
      </Link>
    </div>
  );
}
