import { Link, useLocation } from "react-router-dom";
import BookLogo from "./BookLogo";

export default function TopBar() {
  const location = useLocation();

  return (
    <div className="top-bar">
      <Link to="/" className="logo" style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <BookLogo size={28} />
        <span>EpubReader</span>
      </Link>
      <div className="nav-links">
        <Link
          to="/"
          className={`nav-link ${location.pathname === "/" ? "active" : ""}`}
        >
          书架
        </Link>
        <Link
          to="/settings"
          className={`nav-link ${location.pathname === "/settings" ? "active" : ""}`}
        >
          设置
        </Link>
      </div>
    </div>
  );
}
