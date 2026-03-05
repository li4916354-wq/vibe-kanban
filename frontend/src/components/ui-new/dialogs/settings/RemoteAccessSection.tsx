/**
 * Remote Access Section Component
 * Manages remote access, tunnel settings, and Google Authenticator binding
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Globe, Copy, Eye, EyeOff, Loader2, Link2, Unlink, Smartphone } from 'lucide-react';
import { remoteAccessApi, type RemoteAccessStatus } from '@/lib/remoteAccessApi';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
} from './SettingsComponents';
import { PrimaryButton } from '../../primitives/PrimaryButton';

export function RemoteAccessSection() {
  const { t } = useTranslation(['settings', 'common']);

  // Remote access state
  const [remoteStatus, setRemoteStatus] = useState<RemoteAccessStatus | null>(null);
  const [isEnablingRemote, setIsEnablingRemote] = useState(false);
  const [isEnablingTunnel, setIsEnablingTunnel] = useState(false);
  const [qrCode, setQrCode] = useState<string | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [isEditingPassword, setIsEditingPassword] = useState(false);
  const [customPassword, setCustomPassword] = useState('');
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [isSavingPassword, setIsSavingPassword] = useState(false);

  // TOTP (Google Authenticator) state
  const [totpStatus, setTotpStatus] = useState<{ bound: boolean; secret?: string } | null>(null);
  const [isBindingTotp, setIsBindingTotp] = useState(false);
  const [totpQrCode, setTotpQrCode] = useState<string | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [totpError, setTotpError] = useState<string | null>(null);
  const [showBindingFlow, setShowBindingFlow] = useState(false);

  // Load remote access status
  const loadRemoteStatus = useCallback(async () => {
    try {
      const response = await remoteAccessApi.getStatus();
      if (response.success && response.data) {
        setRemoteStatus(response.data);
      }
    } catch (error) {
      console.error('[RemoteAccessSection] loadRemoteStatus error:', error);
    }
  }, []);

  // Load TOTP status
  const loadTotpStatus = useCallback(async () => {
    try {
      const response = await remoteAccessApi.getTotpStatus();
      if (response.success && response.data) {
        setTotpStatus(response.data);
      }
    } catch (error) {
      console.error('[RemoteAccessSection] loadTotpStatus error:', error);
    }
  }, []);

  useEffect(() => {
    loadRemoteStatus();
    loadTotpStatus();

    // Subscribe to status changes
    const unsubscribe = remoteAccessApi.onStatusChange((data) => {
      setRemoteStatus(data);
    });

    return () => {
      unsubscribe();
    };
  }, [loadRemoteStatus, loadTotpStatus]);

  // Load QR code when remote is enabled
  useEffect(() => {
    if (remoteStatus?.enabled) {
      loadQRCode();
    } else {
      setQrCode(null);
    }
  }, [remoteStatus?.enabled, remoteStatus?.tunnel.url]);

  const loadQRCode = async () => {
    try {
      const response = await remoteAccessApi.getQRCode(false);
      if (response.success && response.data) {
        setQrCode(response.data.qrCode);
      }
    } catch (error) {
      console.error('[RemoteAccessSection] loadQRCode error:', error);
    }
  };

  const handleToggleRemote = async () => {
    if (remoteStatus?.enabled) {
      const response = await remoteAccessApi.disable();
      if (response.success) {
        setRemoteStatus(null);
        setQrCode(null);
      }
    } else {
      setIsEnablingRemote(true);
      try {
        const response = await remoteAccessApi.enable();
        if (response.success && response.data) {
          setRemoteStatus(response.data);
        }
      } catch {
        // Enable failed silently
      } finally {
        setIsEnablingRemote(false);
      }
    }
  };

  const handleToggleTunnel = async () => {
    if (remoteStatus?.tunnel.status === 'running') {
      await remoteAccessApi.disableTunnel();
    } else {
      setIsEnablingTunnel(true);
      try {
        await remoteAccessApi.enableTunnel();
      } finally {
        setIsEnablingTunnel(false);
      }
    }
    loadRemoteStatus();
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  const handleSavePassword = async () => {
    if (customPassword.length < 4) {
      setPasswordError(t('settings.remoteAccess.password.tooShort'));
      return;
    }
    setIsSavingPassword(true);
    setPasswordError(null);
    try {
      const res = await remoteAccessApi.setPassword(customPassword);
      if (res.success) {
        setIsEditingPassword(false);
        setCustomPassword('');
        loadRemoteStatus();
      } else {
        setPasswordError(res.error || t('settings.remoteAccess.password.saveFailed'));
      }
    } catch {
      setPasswordError(t('settings.remoteAccess.password.saveFailed'));
    } finally {
      setIsSavingPassword(false);
    }
  };

  // TOTP binding flow
  const handleStartBinding = async () => {
    setShowBindingFlow(true);
    setTotpError(null);
    setTotpCode('');
    try {
      const response = await remoteAccessApi.generateTotpSecret();
      if (response.success && response.data) {
        setTotpQrCode(response.data.qrCode);
      } else {
        setTotpError(response.error || t('settings.remoteAccess.totp.generateFailed'));
      }
    } catch {
      setTotpError(t('settings.remoteAccess.totp.generateFailed'));
    }
  };

  const handleBindTotp = async () => {
    if (totpCode.length !== 6) {
      setTotpError(t('settings.remoteAccess.totp.invalidCode'));
      return;
    }
    setIsBindingTotp(true);
    setTotpError(null);
    try {
      const response = await remoteAccessApi.bindTotp(totpCode);
      if (response.success) {
        setShowBindingFlow(false);
        setTotpQrCode(null);
        setTotpCode('');
        loadTotpStatus();
      } else {
        setTotpError(response.error || t('settings.remoteAccess.totp.bindFailed'));
      }
    } catch {
      setTotpError(t('settings.remoteAccess.totp.bindFailed'));
    } finally {
      setIsBindingTotp(false);
    }
  };

  const handleUnbindTotp = async () => {
    try {
      const response = await remoteAccessApi.unbindTotp();
      if (response.success) {
        loadTotpStatus();
      }
    } catch (error) {
      console.error('[RemoteAccessSection] unbindTotp error:', error);
    }
  };

  const handleCancelBinding = () => {
    setShowBindingFlow(false);
    setTotpQrCode(null);
    setTotpCode('');
    setTotpError(null);
  };

  return (
    <>
      {/* Remote Access Card */}
      <SettingsCard
        title={t('settings.remoteAccess.title')}
        description={t('settings.remoteAccess.description')}
      >
        {/* Security Warning */}
        <div className="bg-warning/10 border border-warning/30 rounded-sm p-4">
          <div className="flex items-start gap-3">
            <span className="text-warning text-xl">⚠️</span>
            <div className="text-sm">
              <p className="text-warning font-medium mb-1">
                {t('settings.remoteAccess.securityWarning.title')}
              </p>
              <p className="text-warning/80">
                {t('settings.remoteAccess.securityWarning.description')}
              </p>
            </div>
          </div>
        </div>

        {/* Enable/Disable Toggle */}
        <SettingsCheckbox
          id="remote-access-enabled"
          label={t('settings.remoteAccess.enable.label')}
          description={t('settings.remoteAccess.enable.helper')}
          checked={remoteStatus?.enabled || false}
          onChange={handleToggleRemote}
          disabled={isEnablingRemote}
        />

        {/* Remote Access Details */}
        {remoteStatus?.enabled && (
          <>
            {/* Local Access Info */}
            <div className="bg-secondary/50 rounded-sm p-4 space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-low">
                  {t('settings.remoteAccess.localAddress')}
                </span>
                <div className="flex items-center gap-2">
                  <code className="text-sm bg-primary/10 px-2 py-1 rounded">
                    {remoteStatus.server.localUrl}
                  </code>
                  <button
                    onClick={() => copyToClipboard(remoteStatus.server.localUrl || '')}
                    className="text-low hover:text-normal p-1"
                    title={t('common:buttons.copy')}
                  >
                    <Copy className="size-4" />
                  </button>
                </div>
              </div>

              {remoteStatus.server.lanUrl && (
                <div className="flex items-center justify-between">
                  <span className="text-sm text-low">
                    {t('settings.remoteAccess.lanAddress')}
                  </span>
                  <div className="flex items-center gap-2">
                    <code className="text-sm bg-primary/10 px-2 py-1 rounded">
                      {remoteStatus.server.lanUrl}
                    </code>
                    <button
                      onClick={() => copyToClipboard(remoteStatus.server.lanUrl || '')}
                      className="text-low hover:text-normal p-1"
                      title={t('common:buttons.copy')}
                    >
                      <Copy className="size-4" />
                    </button>
                  </div>
                </div>
              )}

              {/* Password Section */}
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-sm text-low">
                    {t('settings.remoteAccess.password.label')}
                  </span>
                  {!isEditingPassword ? (
                    <div className="flex items-center gap-2">
                      <code className="text-sm bg-primary/10 px-2 py-1 rounded font-mono tracking-wider">
                        {showPassword ? remoteStatus.server.token : '••••••••'}
                      </code>
                      <button
                        onClick={() => setShowPassword(!showPassword)}
                        className="text-low hover:text-normal p-1"
                        title={showPassword ? t('common:buttons.hide') : t('common:buttons.show')}
                      >
                        {showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                      </button>
                      <button
                        onClick={() => copyToClipboard(remoteStatus.server.token || '')}
                        className="text-low hover:text-normal p-1"
                        title={t('common:buttons.copy')}
                      >
                        <Copy className="size-4" />
                      </button>
                      <button
                        onClick={() => {
                          setIsEditingPassword(true);
                          setCustomPassword('');
                          setPasswordError(null);
                        }}
                        className="text-xs text-brand hover:text-brand/80"
                      >
                        {t('common:buttons.edit')}
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        value={customPassword}
                        onChange={(e) => {
                          setCustomPassword(e.target.value);
                          setPasswordError(null);
                        }}
                        placeholder={t('settings.remoteAccess.password.placeholder')}
                        maxLength={32}
                        className="w-32 px-2 py-1 text-sm bg-secondary rounded border border-border focus:border-brand focus:outline-none"
                      />
                      <PrimaryButton
                        value={isSavingPassword ? t('common:buttons.saving') : t('common:buttons.save')}
                        onClick={handleSavePassword}
                        disabled={isSavingPassword || customPassword.length < 4}
                        actionIcon={isSavingPassword ? 'spinner' : undefined}
                      />
                      <button
                        onClick={() => {
                          setIsEditingPassword(false);
                          setCustomPassword('');
                          setPasswordError(null);
                        }}
                        className="text-xs text-low hover:text-normal"
                      >
                        {t('common:buttons.cancel')}
                      </button>
                    </div>
                  )}
                </div>
                {passwordError && (
                  <p className="text-xs text-error">{passwordError}</p>
                )}
              </div>

              {remoteStatus.clients > 0 && (
                <div className="flex items-center justify-between text-sm">
                  <span className="text-low">
                    {t('settings.remoteAccess.connectedDevices')}
                  </span>
                  <span className="text-success">
                    {t('settings.remoteAccess.deviceCount', { count: remoteStatus.clients })}
                  </span>
                </div>
              )}
            </div>

            {/* Tunnel Section */}
            <div className="pt-4 border-t border-border">
              <div className="flex items-center justify-between mb-3">
                <div>
                  <p className="font-medium text-normal flex items-center gap-2">
                    <Globe className="size-4" />
                    {t('settings.remoteAccess.tunnel.title')}
                  </p>
                  <p className="text-sm text-low">
                    {t('settings.remoteAccess.tunnel.description')}
                  </p>
                </div>
                <PrimaryButton
                  value={
                    isEnablingTunnel
                      ? t('settings.remoteAccess.tunnel.connecting')
                      : remoteStatus.tunnel.status === 'running'
                        ? t('settings.remoteAccess.tunnel.stop')
                        : remoteStatus.tunnel.status === 'starting'
                          ? t('settings.remoteAccess.tunnel.connecting')
                          : t('settings.remoteAccess.tunnel.start')
                  }
                  onClick={handleToggleTunnel}
                  disabled={isEnablingTunnel}
                  variant={remoteStatus.tunnel.status === 'running' ? 'tertiary' : 'primary'}
                  actionIcon={isEnablingTunnel ? 'spinner' : undefined}
                />
              </div>

              {remoteStatus.tunnel.status === 'running' && remoteStatus.tunnel.url && (
                <div className="bg-success/10 border border-success/30 rounded-sm p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-success">
                      {t('settings.remoteAccess.tunnel.publicAddress')}
                    </span>
                    <div className="flex items-center gap-2">
                      <code className="text-sm bg-primary/10 px-2 py-1 rounded text-success">
                        {remoteStatus.tunnel.url}
                      </code>
                      <button
                        onClick={() => copyToClipboard(remoteStatus.tunnel.url || '')}
                        className="text-success/80 hover:text-success p-1"
                        title={t('common:buttons.copy')}
                      >
                        <Copy className="size-4" />
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {remoteStatus.tunnel.status === 'error' && (
                <div className="bg-error/10 border border-error/30 rounded-sm p-3">
                  <p className="text-sm text-error">
                    {t('settings.remoteAccess.tunnel.error')}: {remoteStatus.tunnel.error}
                  </p>
                </div>
              )}
            </div>

            {/* QR Code */}
            {qrCode && (
              <div className="pt-4 border-t border-border">
                <p className="font-medium mb-3 text-normal">
                  {t('settings.remoteAccess.qrCode.title')}
                </p>
                <div className="flex flex-col items-center gap-3">
                  <div className="bg-white p-3 rounded-xl">
                    <img src={qrCode} alt="QR Code" className="w-48 h-48" />
                  </div>
                  <div className="text-center text-sm">
                    <p className="text-low">
                      {t('settings.remoteAccess.qrCode.description')}
                    </p>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </SettingsCard>

      {/* Google Authenticator (TOTP) Card */}
      <SettingsCard
        title={t('settings.remoteAccess.totp.title')}
        description={t('settings.remoteAccess.totp.description')}
      >
        {totpStatus?.bound ? (
          // Already bound - show unbind option
          <div className="space-y-4">
            <div className="flex items-center gap-3 p-4 bg-success/10 border border-success/30 rounded-sm">
              <Smartphone className="size-5 text-success" />
              <div className="flex-1">
                <p className="text-sm font-medium text-success">
                  {t('settings.remoteAccess.totp.bound')}
                </p>
                <p className="text-xs text-success/80">
                  {t('settings.remoteAccess.totp.boundDescription')}
                </p>
              </div>
            </div>
            <PrimaryButton
              value={t('settings.remoteAccess.totp.unbind')}
              onClick={handleUnbindTotp}
              variant="tertiary"
              icon={Unlink}
            />
          </div>
        ) : showBindingFlow ? (
          // Binding flow
          <div className="space-y-4">
            {totpQrCode ? (
              <>
                <div className="text-sm text-low">
                  {t('settings.remoteAccess.totp.scanInstructions')}
                </div>
                <div className="flex flex-col items-center gap-4">
                  <div className="bg-white p-3 rounded-xl">
                    <img src={totpQrCode} alt="TOTP QR Code" className="w-48 h-48" />
                  </div>
                  <div className="w-full max-w-xs space-y-2">
                    <SettingsInput
                      value={totpCode}
                      onChange={setTotpCode}
                      placeholder={t('settings.remoteAccess.totp.codePlaceholder')}
                    />
                    {totpError && (
                      <p className="text-xs text-error">{totpError}</p>
                    )}
                  </div>
                  <div className="flex gap-2">
                    <PrimaryButton
                      value={t('common:buttons.cancel')}
                      onClick={handleCancelBinding}
                      variant="tertiary"
                    />
                    <PrimaryButton
                      value={isBindingTotp ? t('settings.remoteAccess.totp.verifying') : t('settings.remoteAccess.totp.verify')}
                      onClick={handleBindTotp}
                      disabled={isBindingTotp || totpCode.length !== 6}
                      actionIcon={isBindingTotp ? 'spinner' : undefined}
                    />
                  </div>
                </div>
              </>
            ) : (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="size-6 animate-spin text-brand" />
              </div>
            )}
          </div>
        ) : (
          // Not bound - show bind option
          <div className="space-y-4">
            <div className="flex items-center gap-3 p-4 bg-secondary/50 rounded-sm">
              <Smartphone className="size-5 text-low" />
              <div className="flex-1">
                <p className="text-sm font-medium text-normal">
                  {t('settings.remoteAccess.totp.notBound')}
                </p>
                <p className="text-xs text-low">
                  {t('settings.remoteAccess.totp.notBoundDescription')}
                </p>
              </div>
            </div>
            <PrimaryButton
              value={t('settings.remoteAccess.totp.bind')}
              onClick={handleStartBinding}
              icon={Link2}
            />
          </div>
        )}
      </SettingsCard>
    </>
  );
}
