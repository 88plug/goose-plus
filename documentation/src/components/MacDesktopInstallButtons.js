import Link from "@docusaurus/Link";
import { IconDownload } from "@site/src/components/icons/download";

const DesktopInstallButtons = () => {
  return (
    <div>
      <p>Click one of the buttons below to download goose-plus Desktop for macOS:</p>
      <div className="pill-button" style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
        <Link
          className="button button--primary button--lg"
          to="https://github.com/88plug/goose-plus/releases/download/stable/goose-plus-darwin-arm64.zip"
        >
          <IconDownload /> macOS Silicon
        </Link>
        <Link
          className="button button--primary button--lg"
          to="https://github.com/88plug/goose-plus/releases/download/stable/goose-plus-darwin-x64.zip"
        >
          <IconDownload /> macOS Intel
        </Link>
      </div>
    </div>
  );
};

export default DesktopInstallButtons;
