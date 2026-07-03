import { useEffect, useState, useCallback, useRef } from 'react';
import { View } from '../../../utils/navigationUtils';
import ModelSettingsButtons from './subcomponents/ModelSettingsButtons';
import { useConfig } from '../../ConfigContext';
import { modelAndProviderMessages, useModelAndProvider } from '../../ModelAndProviderContext';
import { toastError } from '../../../toasts';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import ResetProviderSection from '../reset_provider/ResetProviderSection';
import { Switch } from '../../ui/switch';
import { trackSettingToggled } from '../../../utils/analytics';
import { defineMessages, useIntl } from '../../../i18n';

export const MODEL_LOCK_USER_PREF_KEY = 'GOOSE_MODEL_LOCK_USER_PREF';
export const MODEL_LOCK_CHANGED_EVENT = 'model-lock-changed';

const i18n = defineMessages({
  resetTitle: {
    id: 'modelsSection.resetTitle',
    defaultMessage: 'Reset Provider and Model',
  },
  resetDescription: {
    id: 'modelsSection.resetDescription',
    defaultMessage: 'Clear your selected model and provider settings to start fresh',
  },
  lockTitle: {
    id: 'modelsSection.lockTitle',
    defaultMessage: 'Lock Model',
  },
  lockDescription: {
    id: 'modelsSection.lockDescription',
    defaultMessage: 'Prevent switching models from the chat bottom bar',
  },
});

interface ModelsSectionProps {
  setView: (view: View) => void;
}

export default function ModelsSection({ setView }: ModelsSectionProps) {
  const intl = useIntl();
  const [provider, setProvider] = useState<string | null>(null);
  const [displayModelName, setDisplayModelName] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [isModelLocked, setIsModelLocked] = useState<boolean>(false);
  const { read, upsert, getProviders } = useConfig();
  const {
    getCurrentModelDisplayName,
    getCurrentProviderDisplayName,
    currentModel,
    currentProvider,
  } = useModelAndProvider();

  const loadModelData = useCallback(async () => {
    try {
      setIsLoading(true);

      // Get display name (alias if available, otherwise model name)
      const modelDisplayName = await getCurrentModelDisplayName();
      setDisplayModelName(modelDisplayName);

      // Get provider display name (subtext if available from predefined models, otherwise provider metadata)
      const providerDisplayName = await getCurrentProviderDisplayName();
      if (providerDisplayName) {
        setProvider(providerDisplayName);
      } else {
        // Fallback to original provider lookup
        const gooseProvider = (await read('GOOSE_PROVIDER', false)) as string;
        const providers = await getProviders(true);
        const providerDetailsList = providers.filter((provider) => provider.name === gooseProvider);

        if (providerDetailsList.length != 1) {
          toastError({
            title: intl.formatMessage(modelAndProviderMessages.unknownProviderTitle),
            msg: intl.formatMessage(modelAndProviderMessages.unknownProviderMsg),
          });
          setProvider(gooseProvider);
        } else {
          const fallbackProviderDisplayName = providerDetailsList[0].metadata.display_name;
          setProvider(fallbackProviderDisplayName);
        }
      }
    } catch (error) {
      console.error('Error loading model data:', error);
    } finally {
      setIsLoading(false);
    }
  }, [read, getProviders, getCurrentModelDisplayName, getCurrentProviderDisplayName, intl]);

  useEffect(() => {
    loadModelData();
  }, [loadModelData]);

  useEffect(() => {
    const loadLockState = async () => {
      try {
        const savedState = await read(MODEL_LOCK_USER_PREF_KEY, false);
        if (savedState === null || savedState === undefined) {
          // No user preference — fall back to the env-var default.
          const envDefault = window.appConfig.get('GOOSE_MODEL_LOCK');
          setIsModelLocked(envDefault === true);
        } else {
          setIsModelLocked(savedState === true || savedState === 'true');
        }
      } catch (error) {
        console.error('Error loading model lock state:', error);
        setIsModelLocked(false);
      }
    };
    loadLockState();
  }, [read]);

  const handleLockToggle = async (checked: boolean) => {
    setIsModelLocked(checked);
    await upsert(MODEL_LOCK_USER_PREF_KEY, checked, false);
    trackSettingToggled('model_locked', checked);
    window.dispatchEvent(new CustomEvent(MODEL_LOCK_CHANGED_EVENT));
  };

  // Update display when model or provider changes - but only if they actually changed
  const prevModelRef = useRef<string | null>(null);
  const prevProviderRef = useRef<string | null>(null);

  useEffect(() => {
    if (
      currentModel &&
      currentProvider &&
      (currentModel !== prevModelRef.current || currentProvider !== prevProviderRef.current)
    ) {
      prevModelRef.current = currentModel;
      prevProviderRef.current = currentProvider;
      loadModelData();
    }
  }, [currentModel, currentProvider, loadModelData]);

  return (
    <section id="models" className="space-y-4 pr-4">
      <Card className="p-2 pb-4">
        <CardContent className="px-2">
          {isLoading ? (
            <>
              <div className="h-[20px] mb-1"></div>
              <div className="h-[16px]"></div>
            </>
          ) : (
            <div className="animate-in fade-in duration-100">
              <h3 className="text-text-primary">{displayModelName}</h3>
              <h4 className="text-xs text-text-secondary">{provider}</h4>
            </div>
          )}
          <ModelSettingsButtons setView={setView} />
        </CardContent>
      </Card>
      <Card className="pb-2 rounded-lg">
        <CardContent className="px-2 pt-2">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.lockTitle)}</h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.lockDescription)}
              </p>
            </div>
            <div className="flex items-center">
              <Switch checked={isModelLocked} onCheckedChange={handleLockToggle} variant="mono" />
            </div>
          </div>
        </CardContent>
      </Card>
      <Card className="pb-2 rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="">{intl.formatMessage(i18n.resetTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.resetDescription)}</CardDescription>
        </CardHeader>
        <CardContent className="px-2">
          <ResetProviderSection setView={setView} />
        </CardContent>
      </Card>
    </section>
  );
}
