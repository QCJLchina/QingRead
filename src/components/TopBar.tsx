import { Link, useLocation } from "react-router-dom";
import BookLogo from "./BookLogo";

export default function TopBar() {
  const location = useLocation();

  return (
    <header className="top-bar">
      <Link to="/" className="logo">
        <BookLogo size={26} />
        <span>轻阅</span>
      </Link>
      <nav className="nav-links" aria-label="主导航">
        <Link to="/" className={"nav-link" + (location.pathname === "/" ? " is-active" : "")}>
          书架
        </Link>
        <Link
          to="/settings"
          className={"nav-link" + (location.pathname === "/settings" ? " is-active" : "")}
        >
          设置
        </Link>
      </nav>
    </header>
  );
}
