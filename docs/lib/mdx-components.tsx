import { File, Files, Folder } from 'fumadocs-ui/components/files';
import { Step, Steps } from 'fumadocs-ui/components/steps';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';
import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { Availability } from '@/components/docs/availability';
import { ExpectedOutput } from '@/components/docs/expected-output';
import { LifecycleDiagram } from '@/components/docs/lifecycle-diagram';

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    Availability,
    ExpectedOutput,
    LifecycleDiagram,
    File,
    Files,
    Folder,
    Step,
    Steps,
    Tab,
    Tabs,
    ...components,
  };
}
