#[doc = r" The RTIC application module"] pub mod app
{
    #[doc =
    r" Always include the device crate which contains the vector table"] use
    stm32f4xx_hal :: pac as
    you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml;
    #[doc =
    r" Holds the maximum priority level for use by async HAL drivers."]
    #[no_mangle] static RTIC_ASYNC_MAX_LOGICAL_PRIO : u8 = 1 << stm32f4xx_hal
    :: pac :: NVIC_PRIO_BITS; use super :: * ; #[doc = r" User code end"]
    #[doc = r"Shared resources"] struct Shared { sample : Sample, }
    #[doc = r"Local resources"] struct Local {} #[doc = r" Execution context"]
    #[allow(non_snake_case)] #[allow(non_camel_case_types)] pub struct
    __rtic_internal_init_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > ,
        #[doc = r" The space used to allocate async executors in bytes."] pub
        executors_size : usize, #[doc = r" Core peripherals"] pub core : rtic
        :: export :: Peripherals, #[doc = r" Device peripherals (PAC)"] pub
        device : stm32f4xx_hal :: pac :: Peripherals,
        #[doc = r" Critical section token for init"] pub cs : rtic :: export
        :: CriticalSection < 'a > ,
    } impl < 'a > __rtic_internal_init_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn
        new(core : rtic :: export :: Peripherals, executors_size : usize) ->
        Self
        {
            __rtic_internal_init_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, core :
                core, device : stm32f4xx_hal :: pac :: Peripherals :: steal(),
                cs : rtic :: export :: CriticalSection :: new(),
                executors_size,
            }
        }
    } #[allow(non_snake_case)] #[doc = "Initialization function"] pub mod init
    {
        #[doc(inline)] pub use super :: __rtic_internal_init_Context as
        Context;
    } #[inline(always)] #[allow(non_snake_case)] fn
    init(_cx : init :: Context) -> (Shared, Local)
    {
        first :: spawn().unwrap(); second :: spawn().unwrap();
        (Shared { sample : Sample(0) }, Local {})
    } impl < 'a > __rtic_internal_firstSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_firstSharedResources
            {
                sample : shared_resources :: sample_that_needs_to_be_locked ::
                new(), __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } impl < 'a > __rtic_internal_secondSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_secondSharedResources
            {
                sample : shared_resources :: sample_that_needs_to_be_locked ::
                new(), __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `first` has access to"] pub struct
    __rtic_internal_firstSharedResources < 'a >
    {
        #[allow(missing_docs)] pub sample : shared_resources ::
        sample_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Spawns the task directly"] #[allow(non_snake_case)]
    #[doc(hidden)] #[allow(clippy :: extra_unused_lifetimes)] pub fn
    __rtic_internal_first_spawn < 'non_static > () -> :: core :: result ::
    Result < (), () >
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(first, & __rtic_internal_first_EXEC); if
            exec.try_allocate()
            {
                let future = first(unsafe { first :: Context :: new() });
                exec.spawn(future); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1); Ok(())
            } else { Err(()) }
        }
    } #[doc = r" Gives waker to the task"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_first_waker() -> :: core :: task ::
    Waker
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(first, & __rtic_internal_first_EXEC);
            exec.waker(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(first, & __rtic_internal_first_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1);
            })
        }
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_first_Context <
    'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Shared Resources this task has access to"] pub
        shared : first :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_first_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_first_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, shared :
                first :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Software task"] pub mod first
    {
        #[doc(inline)] pub use super :: __rtic_internal_firstSharedResources
        as SharedResources; #[doc(inline)] pub use super ::
        __rtic_internal_first_spawn as spawn; #[doc(inline)] pub use super ::
        __rtic_internal_first_waker as waker; #[doc(inline)] pub use super ::
        __rtic_internal_first_Context as Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `second` has access to"] pub struct
    __rtic_internal_secondSharedResources < 'a >
    {
        #[allow(missing_docs)] pub sample : shared_resources ::
        sample_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Spawns the task directly"] #[allow(non_snake_case)]
    #[doc(hidden)] #[allow(clippy :: extra_unused_lifetimes)] pub fn
    __rtic_internal_second_spawn < 'non_static > () -> :: core :: result ::
    Result < (), () >
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(second, & __rtic_internal_second_EXEC); if
            exec.try_allocate()
            {
                let future = second(unsafe { second :: Context :: new() });
                exec.spawn(future); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1); Ok(())
            } else { Err(()) }
        }
    } #[doc = r" Gives waker to the task"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_second_waker() -> :: core :: task ::
    Waker
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(second, & __rtic_internal_second_EXEC);
            exec.waker(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(second, & __rtic_internal_second_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1);
            })
        }
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_second_Context <
    'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Shared Resources this task has access to"] pub
        shared : second :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_second_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_second_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, shared :
                second :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Software task"] pub mod second
    {
        #[doc(inline)] pub use super :: __rtic_internal_secondSharedResources
        as SharedResources; #[doc(inline)] pub use super ::
        __rtic_internal_second_spawn as spawn; #[doc(inline)] pub use super ::
        __rtic_internal_second_waker as waker; #[doc(inline)] pub use super ::
        __rtic_internal_second_Context as Context;
    } #[allow(non_snake_case)] async fn first < 'a >
    (mut cx : first :: Context < 'a >)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ;
        cx.shared.sample.lock(update);
    } #[allow(non_snake_case)] async fn second < 'a >
    (mut cx : second :: Context < 'a >)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ;
        cx.shared.sample.lock(update);
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic0"] static
    __rtic_internal_shared_resource_sample : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Sample >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: sample_that_needs_to_be_locked < 'a >
    {
        type T = Sample; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut Sample) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 1u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_sample.get_mut() as * mut
                _, CEILING, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS, f,)
            }
        }
    } mod shared_resources
    {
        #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        sample_that_needs_to_be_locked < 'a >
        {
            __rtic_internal_p : :: core :: marker :: PhantomData <
            (& 'a (), * const u8) > ,
        } unsafe impl < 'a > Sync for sample_that_needs_to_be_locked < 'a > {}
        impl < 'a > sample_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new() -> Self
            {
                sample_that_needs_to_be_locked
                { __rtic_internal_p : :: core :: marker :: PhantomData }
            }
        }
    } #[allow(non_upper_case_globals)] static __rtic_internal_first_EXEC :
    rtic :: export :: executor :: AsyncTaskExecutorPtr = rtic :: export ::
    executor :: AsyncTaskExecutorPtr :: new();
    #[allow(non_upper_case_globals)] static __rtic_internal_second_EXEC : rtic
    :: export :: executor :: AsyncTaskExecutorPtr = rtic :: export :: executor
    :: AsyncTaskExecutorPtr :: new(); #[allow(non_snake_case)]
    #[doc = "Interrupt handler to dispatch async tasks at priority 1"]
    #[no_mangle] unsafe fn USART1()
    {
        #[doc = r" The priority of this interrupt handler"] const PRIORITY :
        u8 = 1u8; rtic :: export ::
        run(PRIORITY, ||
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(first, & __rtic_internal_first_EXEC);
            exec.poll(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(first, & __rtic_internal_first_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1);
            }); let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(second, & __rtic_internal_second_EXEC);
            exec.poll(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(second, & __rtic_internal_second_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: USART1);
            });
        });
    } #[doc(hidden)] #[no_mangle] unsafe extern "C" fn main() -> !
    {
        rtic :: export :: assert_send :: < Sample > (); rtic :: export ::
        interrupt :: disable(); let mut core : rtic :: export :: Peripherals =
        rtic :: export :: Peripherals :: steal().into(); let _ =
        you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml ::
        interrupt :: USART1; const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 1u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'USART1' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: USART1, rtic :: export ::
        cortex_logical2hw(1u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: USART1); #[inline(never)] fn __rtic_init_resources < F
        > (f : F) where F : FnOnce() { f(); } let mut executors_size = 0; let
        executor = :: core :: mem :: ManuallyDrop ::
        new(rtic :: export :: executor :: AsyncTaskExecutor ::
        new_1_args(first));
        {
            executors_size += :: core :: mem :: size_of_val(& executor);
            __rtic_internal_first_EXEC.set_in_main(& executor);
        } let executor = :: core :: mem :: ManuallyDrop ::
        new(rtic :: export :: executor :: AsyncTaskExecutor ::
        new_1_args(second));
        {
            executors_size += :: core :: mem :: size_of_val(& executor);
            __rtic_internal_second_EXEC.set_in_main(& executor);
        } extern "C"
        { pub static _stack_start : u32; pub static __ebss : u32; } let
        stack_start = & _stack_start as * const _ as u32; let ebss = & __ebss
        as * const _ as u32; if stack_start > ebss
        {
            if rtic :: export :: msp :: read() <= ebss
            {
                :: core :: panic!
                ("Stack overflow after allocating executors");
            }
        }
        __rtic_init_resources(||
        {
            let (shared_resources, local_resources) =
            init(init :: Context :: new(core.into(), executors_size));
            __rtic_internal_shared_resource_sample.get_mut().write(core :: mem
            :: MaybeUninit :: new(shared_resources.sample)); rtic :: export ::
            interrupt :: enable();
        }); loop {}
    }
}