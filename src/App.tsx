import React, { useEffect } from 'react';
import { useAppStore } from './store';
import Desktop from './components/Desktop';
import Onboarding from './screens/Onboarding';

export default function App() {
  const {
    vaultStatus,
    refreshVaultStatus,
    refreshWorkspaces,
    refreshSettings,
    refreshInboxCount,
    refreshPermissions,
  } = useAppStore();

  // Bootstrap on mount
  useEffect(() => {
    (async () => {
      await refreshVaultStatus();
    })();
  }, []);

  // Load data whenever vault becomes unlocked
  useEffect(() => {
    if (vaultStatus.unlocked) {
      refreshWorkspaces();
      refreshSettings();
      refreshInboxCount();
      refreshPermissions();
    }
  }, [vaultStatus.unlocked]);

  if (!vaultStatus.unlocked) {
    return <Onboarding />;
  }

  return <Desktop />;
}
