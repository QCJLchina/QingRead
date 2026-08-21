interface BookLogoProps {
  size?: number;
  format?: string;
}

export default function BookLogo({ size = 80, format = "epub" }: BookLogoProps) {
  const isTxt = format === "txt";
  const color = isTxt ? "#5B8DEF" : "#3B7DDD";
  const colorLight = isTxt ? "#8BB3F5" : "#6FA8E8";
  const colorDark = isTxt ? "#3A6AD4" : "#2A5FB8";

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* 书本主体 */}
      <rect x="20" y="15" width="60" height="75" rx="4" fill={color} />
      {/* 书脊 */}
      <rect x="16" y="15" width="8" height="75" rx="3" fill={colorDark} />
      {/* 书页 */}
      <rect x="26" y="20" width="48" height="65" rx="2" fill="white" />
      {/* 页面内容线条 */}
      <rect x="32" y="30" width="30" height="3" rx="1.5" fill="#ddd" />
      <rect x="32" y="40" width="36" height="3" rx="1.5" fill="#e8e8e8" />
      <rect x="32" y="50" width="28" height="3" rx="1.5" fill="#e8e8e8" />
      <rect x="32" y="60" width="32" height="3" rx="1.5" fill="#e8e8e8" />
      {/* 右上角折角 */}
      <path d="M66 20 L66 30 L74 30 Z" fill={colorLight} />
      <path d="M66 20 L74 20 L74 28 Z" fill={colorLight} />
      <path d="M66 20 L74 28 L66 28 Z" fill="white" opacity="0.3" />
    </svg>
  );
}
