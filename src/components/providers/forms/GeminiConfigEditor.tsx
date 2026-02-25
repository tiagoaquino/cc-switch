import React from "react";
import {
  GeminiEnvSection,
  GeminiConfigSection,
  GeminiAuthFilesSection,
} from "./GeminiConfigSections";

interface GeminiConfigEditorProps {
  envValue: string;
  configValue: string;
  manageAuthFiles: boolean;
  googleAccountsValue: string;
  oauthCredsValue: string;
  onEnvChange: (value: string) => void;
  onConfigChange: (value: string) => void;
  onManageAuthFilesChange: (enabled: boolean) => void;
  onGoogleAccountsChange: (value: string) => void;
  onOauthCredsChange: (value: string) => void;
  onEnvBlur?: () => void;
  envError: string;
  configError: string;
  googleAccountsError: string;
  oauthCredsError: string;
}

const GeminiConfigEditor: React.FC<GeminiConfigEditorProps> = ({
  envValue,
  configValue,
  manageAuthFiles,
  googleAccountsValue,
  oauthCredsValue,
  onEnvChange,
  onConfigChange,
  onManageAuthFilesChange,
  onGoogleAccountsChange,
  onOauthCredsChange,
  onEnvBlur,
  envError,
  configError,
  googleAccountsError,
  oauthCredsError,
}) => {
  return (
    <div className="space-y-6">
      <GeminiEnvSection
        value={envValue}
        onChange={onEnvChange}
        onBlur={onEnvBlur}
        error={envError}
      />

      <GeminiConfigSection
        value={configValue}
        onChange={onConfigChange}
        configError={configError}
      />

      <GeminiAuthFilesSection
        enabled={manageAuthFiles}
        onEnabledChange={onManageAuthFilesChange}
        googleAccountsValue={googleAccountsValue}
        oauthCredsValue={oauthCredsValue}
        onGoogleAccountsChange={onGoogleAccountsChange}
        onOauthCredsChange={onOauthCredsChange}
        googleAccountsError={googleAccountsError}
        oauthCredsError={oauthCredsError}
      />
    </div>
  );
};

export default GeminiConfigEditor;
